extern crate iron;
extern crate env_logger;
extern crate handlebars_iron as hbs;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;
extern crate flate2;
extern crate staticfile;
extern crate mount;
extern crate urlencoded;
extern crate toml;
extern crate semver;
#[macro_use]
extern crate hyper;
extern crate pulldown_cmark;
extern crate ammonia;
extern crate syntect;
#[macro_use]
extern crate lazy_static;
extern crate failure;

extern crate all_crate_storage;

use iron::prelude::*;
use iron::{AfterMiddleware, Handler, status};
use iron::headers::{ContentEncoding, Encoding, Location};
use hbs::{Template, HandlebarsEngine, DirectorySource};
use hbs::handlebars::to_json;
use serde_json::value::{Value, Map};

use iron::headers::Referer;

use std::time::Duration;
use std::path::Path;
use std::fs::File;
use std::io::Read;
use std::cell::RefCell;
use std::sync::RwLock;
use std::fmt::Display;

use flate2::Compression;
use flate2::write::GzEncoder;

use staticfile::Static;

use mount::Mount;

use urlencoded::UrlEncodedQuery;

use semver::Version as SvVersion;

use all_crate_storage::registry::registry::Registry;
use all_crate_storage::registry::statistics::{compute_crate_statistics, CrateStats};
use all_crate_storage::crate_storage::{DynCrateSource, FileTreeStorage, CrateSource};
use all_crate_storage::blob_crate_storage::BlobCrateStorage;
use all_crate_storage::crate_storage::CrateSpec;

mod registry_data;
mod markdown_render;
mod escape;
mod code_format;
mod syntect_format;

#[derive(Debug)]
pub struct StrErr(String);

impl<T :Display> From<T> for StrErr {
	fn from(v :T) -> Self {
		StrErr(format!("{}", v))
	}
}

impl StrErr {
	fn as_map(&self) -> Map<String, Value> {
		let mut m = Map::new();
		m.insert("error".to_string(), to_json(&self.0));
		m
	}
}

pub struct GzMiddleware;

impl AfterMiddleware for GzMiddleware {
	fn after(&self, req: &mut Request, mut resp: Response) -> IronResult<Response> {
		if req.url.path()[0] == "api" {
			// Don't compress download responses
			return Ok(resp);
		}

		let compressed_bytes = resp.body.as_mut().map(|b| {
			let mut encoder = GzEncoder::new(Vec::new(), Compression::best());
			{
				let _ = b.write_body(&mut encoder);
			}
			encoder.finish().unwrap()
		});

		if let Some(b) = compressed_bytes {
			resp.headers.set(ContentEncoding(vec![Encoding::Gzip]));
			resp.set_mut(b);
		}
		Ok(resp)
	}
}

struct FallbackHandler(Box<dyn Handler>);

impl Handler for FallbackHandler {
	fn handle(&self, req: &mut Request) -> IronResult<Response> {
		let resp = self.0.handle(req);

		match resp {
			Err(err) => {
				match err.response.status {
					Some(status) => {
						let mut m = Map::new();
						m.insert("error".to_string(), Value::from(format!("{}", status)));
						Ok(Response::with((status,
							Template::new("error", m))))
					}
					_ => Err(err),
				}
			}
			other => other
		}
	}
}

lazy_static! {
	static ref REGISTRY :Registry =
		Registry::from_name("github.com-1ecc6299db9ec823").unwrap();
	static ref CRATE_STATS :CrateStats =
		compute_crate_statistics(&REGISTRY.get_all_crates_json().unwrap());
	static ref CRATE_SOURCE_GEN :RwLock<Option<Box<dyn Fn() -> DynCrateSource<File> + Send + Sync>>> = RwLock::new(None);
}

thread_local!(static CRATE_SOURCE :RefCell<DynCrateSource<File>> = {
	let csg = CRATE_SOURCE_GEN.read().unwrap();
	let dcs = csg.as_ref().unwrap()();
	RefCell::new(dcs)
});

header! { (ContentSecurityPolicy, "Content-Security-Policy") => [String] }

fn csp_hdr(req :&mut Request, mut res :Response) -> IronResult<Response> {
	let mut csp_header =
		"default-src 'none'; \
		img-src 'self'; \
		form-action 'self'; ".to_owned();

	let path = req.url.path();
	let allow_inline_style = if let Some(z) = path.get(0) {
		// TODO find a way to avoid inline css in the syntect formatter
		// and then remove || z == &"crate".
		// https://github.com/trishume/syntect/issues/121
		if z == &"static" || z == &"crate" || z == &"files" {
			// Needed for inline CSS inside SVG
			true
		} else {
			false
		}
	} else {
		false
	};
	if allow_inline_style {
		csp_header += "style-src 'self' 'unsafe-inline';";
	} else {
		csp_header += "style-src 'self';";
	}
	res.headers.set(ContentSecurityPolicy(csp_header));
	Ok(res)
}

fn krate(r: &mut Request) -> IronResult<Response> {
	let path = r.url.path();
	let name = path[0];
	let opt_version = path.get(1).map(|v| *v);
	let mut resp = Response::new();
	CRATE_SOURCE.with(|s| {
		let data = registry_data::get_crate_data(name.to_string(),
			&REGISTRY, &mut *s.borrow_mut(), opt_version);
		match data {
			Ok(d) => {
				resp.set_mut(Template::new("crate", d))
					.set_mut(status::Ok);
			},
			Err(e) => {
				resp.set_mut(Template::new("error", e.as_map()))
					.set_mut(status::Ok);
			},
		}
	});
	Ok(resp)
}

fn versions(r: &mut Request) -> IronResult<Response> {
	let path = r.url.path();
	let name = path[0];
	let mut resp = Response::new();

	let refferer = r.headers.get::<Referer>()
		.map(|s| s.as_str().to_string());

	let crate_data = registry_data::get_versions_data(name, &REGISTRY, refferer);
	resp.set_mut(Template::new("versions", crate_data))
		.set_mut(status::Ok);
	Ok(resp)
}

fn reverse_dependencies(r: &mut Request) -> IronResult<Response> {
	let path = r.url.path();
	let name = path[0];
	let mut resp = Response::new();

	let refferer = r.headers.get::<Referer>()
		.map(|s| s.as_str().to_string());

	let only_latest_versions = false;

	let crate_data = registry_data::get_reverse_dependencies(name,
		only_latest_versions, &CRATE_STATS, refferer);
	resp.set_mut(Template::new("reverse_dependencies", crate_data))
		.set_mut(status::Ok);
	Ok(resp)
}

fn index(_: &mut Request) -> IronResult<Response> {
	let mut resp = Response::new();

	let crate_data = registry_data::get_index_data(&CRATE_STATS);
	resp.set_mut(Template::new("index", crate_data))
		.set_mut(status::Ok);
	Ok(resp)
}

fn search(req :&mut Request) -> IronResult<Response> {
	let mut resp = Response::new();

	let hmap = req.get_ref::<UrlEncodedQuery>().unwrap();
	let (crate_data, maybe_only_one) = registry_data::get_search_result_data(&CRATE_STATS, hmap);
	if let Some(only_crate_name) = maybe_only_one {
		resp.headers.set(Location(format!("/crate/{}", only_crate_name)));
		resp.set_mut(status::Found);
	} else {
		resp.set_mut(Template::new("search", crate_data))
			.set_mut(status::Ok);
	}
	Ok(resp)
}

fn crate_files(req :&mut Request) -> IronResult<Response> {
	use self::registry_data::CrateFileData::*;

	let path = req.url.path();
	let name = path[0];
	let version = path[1];
	let mut resp = Response::new();

	CRATE_SOURCE.with(|s| {
		let crate_file_data = registry_data::get_crate_file_data(
			&mut *s.borrow_mut(), name, version, &path[2..]);
		let template = match crate_file_data {
			FileListing(data) => Template::new("file-listing", data),
			FileContent(data) => Template::new("file-content", data),
		};
		resp.set_mut(template)
			.set_mut(status::Ok);
	});
	Ok(resp)
}

fn api_crate(req :&mut Request) -> IronResult<Response> {

	println!("{:?}", req.url.path());
	let path = req.url.path();
	let name = path[0];
	let version = path[1];
	let mut resp = Response::new();

	let sv_version = SvVersion::parse(version).unwrap();
	let crate_spec = CrateSpec {
		name : name.to_string(),
		version : sv_version,
	};
	CRATE_SOURCE.with(|s| {
		let s = &mut *s.borrow_mut();
		let crate_opt = s.get_crate(&crate_spec);
		if let Some(crate_data) = crate_opt {
			use all_crate_storage::reconstruction::CrateContentBlobs;
			let ccb = CrateContentBlobs::from_archive_file(&crate_data as &[u8]).unwrap();
			let crate_data = ccb.to_archive_file();
			resp.set_mut(crate_data)
				.set_mut(status::Ok);
		} else {
			resp.set_mut(status::NotFound);
		}
	});
	Ok(resp)
}

#[derive(Deserialize, Debug)]
#[serde(tag = "kind")]
enum CrateSourceCfg {
	Cache,
	ArchiveTree {
		path :Option<String>,
	},
	StorageFile {
		path :Option<String>,
	},
}

#[derive(Deserialize, Debug)]
struct AppConfigOpt {
	site_dir :Option<String>,
	listen_host :Option<String>,
	listen_port :Option<u32>,
	source :Option<CrateSourceCfg>,
}

// This construct with AppConfig and AppConfigOpt
// is needed due to
// https://github.com/serde-rs/serde/issues/368
struct AppConfig {
	site_dir :Option<String>,
	listen_host :String,
	listen_port :u32,
	source :CrateSourceCfg,
}

impl AppConfig {
	pub fn from_opt(o :AppConfigOpt) -> Self {
		AppConfig {
			site_dir : o.site_dir,
			listen_host : o.listen_host.unwrap_or("localhost".to_owned()),
			listen_port : o.listen_port.unwrap_or(3000),
			source : o.source.unwrap_or(CrateSourceCfg::Cache),
		}
	}
}

fn main() {
	env_logger::init();

	let cfg_opt :AppConfigOpt = match File::open("config.toml") {
		Ok(mut f) => {
			let mut s = String::new();
			f.read_to_string(&mut s).unwrap();
			toml::from_str(&s).unwrap()
		},
		Err(_) => {
			toml::from_str("").unwrap()
		},
	};
	//println!("Config: {:?}", cfg_opt);
	let cfg = AppConfig::from_opt(cfg_opt);
	//println!("Config: {:?}", cfg);

	let mut hbse = HandlebarsEngine::new();


	let site_dir :&str = if let Some(d) = cfg.site_dir.as_ref() {
		d
	} else {
		const L :&[&str] = &[
			"./site/",
			"./cargo-local-serve/site/",
			"../cargol-local-serve/site/",
		];
		'a :loop {
			for p in L {
				if Path::new(&p).exists() {
					break 'a p;
				}
			}
			panic!("No valid directory could be found");
		}
	};

	let template_dir = site_dir.to_owned() + "templates/";
	let static_dir = site_dir.to_owned() + "/static/";

	{
		let mut csg = CRATE_SOURCE_GEN.write().unwrap();
		let b = match cfg.source {
			CrateSourceCfg::Cache => Box::new(|| {
				DynCrateSource::CacheStorage(REGISTRY.get_cache_storage())
			}) as Box<dyn Fn() -> DynCrateSource<File> + Send + Sync>,
			CrateSourceCfg::ArchiveTree { path } => {
				let p = if let Some(p) = path {
					p
				} else {
					String::from("crate-archives")
				};
				Box::new(move || {
					DynCrateSource::FileTreeStorage(FileTreeStorage::new(Path::new(&p)))
				}) as Box<dyn Fn() -> DynCrateSource<File> + Send + Sync>
			},
			CrateSourceCfg::StorageFile { path } => {
				let p = if let Some(p) = path {
					p
				} else {
					String::from("crate-constr-archives/crate_storage")
				};
				Box::new(move || {
					let f = File::open(&p).unwrap();
					let bcs = BlobCrateStorage::new(f).unwrap();
					let dcs :DynCrateSource<File> =  DynCrateSource::BlobCrateStorage(bcs);
					dcs
				}) as Box<dyn Fn() -> DynCrateSource<File> + Send + Sync>
			},
		};
		*csg = Some(b);
	}

	// add a directory source, all files with .hbs suffix will be loaded as template
	let template_dir :&str = &template_dir;
	hbse.add(Box::new(DirectorySource::new(template_dir, ".hbs")));

	// load templates from all registered sources
	if let Err(r) = hbse.reload() {
		panic!("{}", r);
	}

	let mut mount = Mount::new();
	mount.mount("/reverse_dependencies", reverse_dependencies);
	mount.mount("/versions", versions);
	mount.mount("/crate", krate);
	mount.mount("/static", Static::new(Path::new(&static_dir))
		.cache(Duration::from_secs(30 * 24 * 60 * 60)));
	mount.mount("/search", search);
	mount.mount("/files", crate_files);
	mount.mount("/api/v1/crates", api_crate);
	mount.mount("/", index);
	let mut chain = Chain::new(FallbackHandler(Box::new(mount)));
	chain.link_after(hbse);
	chain.link_after(csp_hdr);
	chain.link_after(GzMiddleware);
	let host = format!("{}:{}", cfg.listen_host, cfg.listen_port);
	println!("Server running at http://{}/", host);
	Iron::new(chain).http(&host).unwrap();
}
