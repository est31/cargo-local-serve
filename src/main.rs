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
extern crate tar;
extern crate toml;
extern crate semver;
#[macro_use]
extern crate hyper;
extern crate pulldown_cmark;
extern crate ammonia;
extern crate syntect;
#[macro_use]
extern crate lazy_static;
extern crate string_interner;

use iron::prelude::*;
use iron::{AfterMiddleware, Handler, status};
use iron::headers::{ContentEncoding, Encoding};
use hbs::{Template, HandlebarsEngine, DirectorySource};
use hbs::handlebars::{Handlebars, RenderContext, RenderError, Helper};
use serde_json::value::{Value, Map};

use iron::headers::Referer;

use std::time::Duration;
use std::path::Path;

use flate2::Compression;
use flate2::write::GzEncoder;

use staticfile::Static;

use mount::Mount;

use urlencoded::UrlEncodedQuery;

use registry::registry::Registry;
use registry::statistics::{compute_crate_statistics, CrateStats};

mod registry;
mod markdown_render;
mod escape;
mod code_format;

pub struct GzMiddleware;

impl AfterMiddleware for GzMiddleware {
	fn after(&self, _: &mut Request, mut resp: Response) -> IronResult<Response> {

		let compressed_bytes = resp.body.as_mut().map(|b| {
			let mut encoder = GzEncoder::new(Vec::new(), Compression::Best);
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
		if z == &"static" || z == &"crate" {
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
	let crate_data = registry::get_crate_data(name.to_string(), &REGISTRY, opt_version);
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

	let crate_data = registry::get_versions_data(name, &REGISTRY, refferer);
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

	let crate_data = registry::get_reverse_dependencies(name,
		only_latest_versions, &CRATE_STATS, refferer);
	resp.set_mut(Template::new("reverse_dependencies", crate_data))
		.set_mut(status::Ok);
	Ok(resp)
}

fn index(_: &mut Request) -> IronResult<Response> {
	let mut resp = Response::new();

	let crate_data = registry::get_index_data(&CRATE_STATS);
	resp.set_mut(Template::new("index", crate_data))
		.set_mut(status::Ok);
	Ok(resp)
}

fn search(req :&mut Request) -> IronResult<Response> {
	let mut resp = Response::new();

	let hmap = req.get_ref::<UrlEncodedQuery>().unwrap();
	let crate_data = registry::get_search_result_data(&CRATE_STATS, hmap);
	resp.set_mut(Template::new("search", crate_data))
		.set_mut(status::Ok);
	Ok(resp)
}

fn main() {
	env_logger::init().unwrap();

	let mut hbse = HandlebarsEngine::new();

	// add a directory source, all files with .hbs suffix will be loaded as template
	hbse.add(Box::new(DirectorySource::new("./site/templates/", ".hbs")));

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
	mount.mount("/", index);
	let mut chain = Chain::new(FallbackHandler(Box::new(mount)));
	chain.link_after(hbse);
	chain.link_after(csp_hdr);
	chain.link_after(GzMiddleware);
	println!("Server running at http://localhost:3000/");
	Iron::new(chain).http("localhost:3000").unwrap();
}
