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

extern crate all_crates_storage;

use iron::prelude::*;
use iron::{AfterMiddleware, Handler, status};
use iron::headers::{ContentEncoding, Encoding};
use hbs::{Template, HandlebarsEngine, DirectorySource};
use hbs::handlebars::{Handlebars, RenderContext, RenderError, Helper};
use serde_json::value::{Value, Map};

use iron::headers::Referer;

use std::time::Duration;
use std::path::Path;
use std::fs::File;
use std::io::Read;

use flate2::Compression;
use flate2::write::GzEncoder;

use staticfile::Static;

use mount::Mount;

use urlencoded::UrlEncodedQuery;

use all_crates_storage::registry::registry::Registry;
use all_crates_storage::registry::statistics::{compute_crate_statistics, CrateStats};

mod registry_data;
mod markdown_render;
mod escape;
mod code_format;
mod syntect_format;

pub struct GzMiddleware;

impl AfterMiddleware for GzMiddleware {
	fn after(&self, _: &mut Request, mut resp: Response) -> IronResult<Response> {

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

struct FallbackHandler(Box<Handler>);

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
}

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
	let crate_data = registry_data::get_crate_data(name.to_string(),
		&REGISTRY, &mut REGISTRY.get_cache_storage(), opt_version);
	if let Some(data) = crate_data {
		resp.set_mut(Template::new("crate", data))
			.set_mut(status::Ok);
	} else {
		resp.set_mut(status::NotFound);
	}
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
	let crate_data = registry_data::get_search_result_data(&CRATE_STATS, hmap);
	resp.set_mut(Template::new("search", crate_data))
		.set_mut(status::Ok);
	Ok(resp)
}

fn crate_files(req :&mut Request) -> IronResult<Response> {
	use self::registry_data::CrateFileData::*;

	let path = req.url.path();
	let name = path[0];
	let version = path[1];
	let mut resp = Response::new();

	let crate_file_data = registry_data::get_crate_file_data(
		&mut REGISTRY.get_cache_storage(), name, version, &path[2..]);
	let template = match crate_file_data {
		FileListing(data) => Template::new("file-listing", data),
		FileContent(data) => Template::new("file-content", data),
	};
	resp.set_mut(template)
		.set_mut(status::Ok);
	Ok(resp)
}

#[derive(Deserialize)]
struct AppConfigOpt {
	template_dir :Option<String>,
	listen_host :Option<String>,
	listen_port :Option<u32>,
}

// This construct with AppConfig and AppConfigOpt
// is needed due to
// https://github.com/serde-rs/serde/issues/368
struct AppConfig {
	template_dir :Option<String>,
	listen_host :String,
	listen_port :u32,
}

impl AppConfig {
	pub fn from_opt(o :AppConfigOpt) -> Self {
		AppConfig {
			template_dir : o.template_dir,
			listen_host : o.listen_host.unwrap_or("localhost".to_owned()),
			listen_port : o.listen_port.unwrap_or(3000),
		}
	}
}

fn main() {
	env_logger::init().unwrap();

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
	let cfg = AppConfig::from_opt(cfg_opt);

	let mut hbse = HandlebarsEngine::new();


	let template_dir :&str = if let Some(d) = cfg.template_dir.as_ref() {
		d
	} else {
		const L :&[&str] = &[
			"./site/templates/",
			"cargo_local_serve/site/templates/",
			"../cargo_local_serve/site/templates/",
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
	// add a directory source, all files with .hbs suffix will be loaded as template
	hbse.add(Box::new(DirectorySource::new(template_dir, ".hbs")));

	// load templates from all registered sources
	if let Err(r) = hbse.reload() {
		panic!("{}", r);
	}

    hbse.handlebars_mut().register_helper("some_helper",
		Box::new(|_: &Helper,
			_: &Handlebars,
			_: &mut RenderContext| -> Result<(), RenderError> {
			Ok(())
			}
		)
	);

	let mut mount = Mount::new();
	mount.mount("/reverse_dependencies", reverse_dependencies);
	mount.mount("/versions", versions);
	mount.mount("/crate", krate);
	mount.mount("/static", Static::new(Path::new("./site/static"))
		.cache(Duration::from_secs(30 * 24 * 60 * 60)));
	mount.mount("/search", search);
	mount.mount("/files", crate_files);
	mount.mount("/", index);
	let mut chain = Chain::new(FallbackHandler(Box::new(mount)));
	chain.link_after(hbse);
	chain.link_after(csp_hdr);
	chain.link_after(GzMiddleware);
	let host = format!("{}:{}", cfg.listen_host, cfg.listen_port);
	println!("Server running at http://{}", host);
	Iron::new(chain).http(&host).unwrap();
}
