use hbs::handlebars::to_json;
use serde_json::value::{Value, Map};
use flate2::FlateReadExt;
use tar::Archive;
use std::io::Read;
use toml;
use semver::Version as SvVersion;

pub mod registry;

use self::registry::Registry;

#[derive(Serialize, Debug)]
pub struct Crate {
	name :String,
	version :String,
	description :String,
	authors :Vec<Author>,
	license :String,
	versions :Vec<Version>,
	versions_limited :Option<usize>,
	dependencies :Vec<Dependency>,
}

#[derive(Serialize, Debug, PartialEq, Eq)]
pub struct Author {
	name :String,
	email :Option<String>,
}

impl Author {
	pub fn from_str(s :&str) -> Self {
		let mut ssp = s.split('<');
		let name = ssp.next().unwrap().trim().to_string();
		let email = if let Some(snd) = ssp.next() {
			Some(snd.split('>').next().unwrap().to_string())
		} else {
			None
		};
		Author {
			name,
			email,
		}
	}
}

#[test]
fn test_author_generation() {
	assert_eq!(Author::from_str("Hello World <hello@hello.example>"),
		Author{
			name : "Hello World".to_string(),
			email : Some("hello@hello.example".to_string()),
		});
	assert_eq!(Author::from_str("Hello World"),
		Author{
			name : "Hello World".to_string(),
			email : None,
		});
}

#[derive(Serialize, Debug)]
pub struct Version {
	v :String,
	date :Option<String>,
}

#[derive(Serialize, Debug)]
pub struct Dependency {
	name :String,
	version :String,
}

#[allow(dead_code)]
pub fn winapi_crate_data() -> Map<String, Value> {
	let mut data = Map::new();

	let krate = Crate {
		name : "winapi".to_string(),
		version : "0.2.8".to_string(),
		description : "Types and constants for WinAPI bindings. See README for list of crates providing function bindings.".to_string(),
		authors : vec![
			Author {
				name : "Peter Atashian".to_string(),
				email : Some("retep998@gmail.com".to_string()),
			},
		],
		license : "MIT".to_string(),
		versions : vec![
			Version {
				v: "0.2.8".to_string(),
				date: Some("Jul 12, 2016".to_string()),
			},
			Version {
				v: "0.2.7".to_string(),
				date: Some("May 10, 2016".to_string()),
			},
			Version {
				v: "0.2.6".to_string(),
				date: Some("Mar 15, 2016".to_string()),
			},
			Version {
				v: "0.2.5".to_string(),
				date: Some("Nov 9, 2015".to_string()),
			},
		],
		versions_limited : None,
		dependencies : vec![],
	};
	data.insert("c".to_string(), to_json(&krate));
	data
}

macro_rules! otry {
	($v:expr) => {{
		if let Some(v) = $v.ok() {
			v
		} else {
			return None;
		}
	}};
}

#[derive(Deserialize)]
struct CratePackage {
	description :String,
	license :String,
	authors :Vec<String>,
}

#[derive(Deserialize)]
struct CrateInfo {
	package :CratePackage,
}

pub fn get_crate_data(name :String, version :Option<&str>)
		-> Option<Map<String, Value>> {
	let mut data = Map::new();

	// First step: find the path to the crate.
	let r = Registry::from_name("github.com-1ecc6299db9ec823").unwrap();
	let crate_json = r.get_crate_json(&name).unwrap();
	let version = if let Some(v) = version {
		SvVersion::parse(v).unwrap()
	} else {
		// Finds the latest version
		// TODO handle the case that there is no version
		// -- then the crate is not present!!!
		crate_json.iter().map(|v| &v.version).max().unwrap().clone()
	};
	let f = match r.get_crate_file(&name, &version).ok() {
		Some(f) => f,
		None => panic!("Version {} of crate {} not mirrored", version, name),
	};
	let decoded = otry!(f.gz_decode());
	let mut archive = Archive::new(decoded);
	let cargo_toml_path = format!("{}-{}/Cargo.toml", name, version);
	let mut cargo_toml_extracted = None;
	for entry in otry!(archive.entries()) {
		let mut entry = otry!(entry);
		let is_cargo_toml = if let Some(path) = otry!(entry.path()).to_str() {
			cargo_toml_path == path
		} else {
			false
		};
		if is_cargo_toml {
			// Extract the Cargo.toml
			let mut v = Vec::new();
			otry!(entry.read_to_end(&mut v));
			cargo_toml_extracted = Some(v);
		}
	}
	let cargo_toml_file = if let Some(toml_file) = cargo_toml_extracted {
		toml_file
	} else {
		return None;
	};
	let versions = crate_json.iter()
		.map(|v| v.version.clone())
		.collect::<Vec<_>>();
	let (v_start, v_limited) = if versions.len() > 4 {
		(versions.len() - 5, true)
	} else {
		(0, false)
	};

	let info :CrateInfo = otry!(toml::from_slice(&cargo_toml_file));

	let krate = Crate {
		name : name.clone(),
		version : version.to_string(),
		description : info.package.description,
		authors : info.package.authors.iter()
			.map(|s| Author::from_str(&s)).collect(),
		license : info.package.license,
		versions : versions[v_start ..].iter().map(|v|
			Version {
				v : format!("{}", v),
				date : None,
			}
		).collect(),
		versions_limited : if v_limited {
			Some(versions.len())
		} else {
			None
		},
		dependencies : vec![],
	};
	data.insert("c".to_string(), to_json(&krate));
	Some(data)
}
