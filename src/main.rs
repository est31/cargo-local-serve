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
extern crate tar;
extern crate toml;
extern crate semver;

use iron::prelude::*;
use iron::status::Status;
use iron::{AfterMiddleware, status};
use iron::headers::{ContentEncoding, Encoding};
use hbs::{Template, HandlebarsEngine, DirectorySource};
use hbs::handlebars::{Handlebars, RenderContext, RenderError, Helper};

use std::time::Duration;
use std::path::Path;

use flate2::Compression;
use flate2::write::GzEncoder;

use staticfile::Static;

use mount::Mount;

mod registry;

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

fn krate(r: &mut Request) -> IronResult<Response> {
	let path = r.url.path();
	let name = path[0];
	let opt_version = path.get(1).map(|v| *v);
	let mut resp = Response::new();
	let crate_data = registry::get_crate_data(name.to_string(), opt_version);
	if let Some(data) = crate_data {
		resp.set_mut(Template::new("crate", data))
			.set_mut(status::Ok);
	} else {
		resp.set_mut(status::NotFound);
	}
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
	mount.mount("/crate", krate);
	mount.mount("/static", Static::new(Path::new("./site/static"))
		.cache(Duration::from_secs(30 * 24 * 60 * 60)));
	let mut chain = Chain::new(mount);
	chain.link_after(hbse);
	chain.link_after(GzMiddleware);
	println!("Server running at http://localhost:3000/");
	Iron::new(chain).http("localhost:3000").unwrap();
}
