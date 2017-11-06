use hbs::handlebars::to_json;
use serde_json::value::{Value, Map};
use flate2::FlateReadExt;
use tar::Archive;
use std::io::{Read, Seek, SeekFrom};
use toml;
use semver::Version as SvVersion;
use pulldown_cmark::{html, Parser};

pub mod registry;

use self::registry::{Registry, DependencyKind};

#[derive(Serialize, Debug)]
pub struct Crate {
	name :String,
	version :String,
	description :String,
	readme_html :Option<String>,
	authors :Vec<Author>,
	license :String,
	versions :Vec<Version>,
	versions_limited :Option<usize>,
	dependencies :Vec<Dependency>,
	dev_dependencies :Option<Vec<Dependency>>,
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
	req :String,
}

#[allow(dead_code)]
pub fn winapi_crate_data() -> Map<String, Value> {
	let mut data = Map::new();

	let krate = Crate {
		name : "winapi".to_string(),
		version : "0.2.8".to_string(),
		description : "Types and constants for WinAPI bindings. See README for list of crates providing function bindings.".to_string(),
		readme_html : None,
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
		dev_dependencies : None,
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
	readme :Option<String>,
}

#[derive(Deserialize)]
struct CrateInfo {
	package :CratePackage,
}

fn extract_path_from_gz<T :Read>(r :T,
		path_ex :&str) -> Option<Vec<u8>> {
	let decoded = if let Some(d) = r.gz_decode().ok() {
		d
	} else {
		return None
	};
	let mut archive = Archive::new(decoded);
	for entry in otry!(archive.entries()) {
		let mut entry = otry!(entry);
		let is_path_ex = if let Some(path) = otry!(entry.path()).to_str() {
			path_ex == path
		} else {
			false
		};
		if is_path_ex {
			// Extract the file
			let mut v = Vec::new();
			otry!(entry.read_to_end(&mut v));
			return Some(v);
		}
	}
	return None;
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
	let mut f = match r.get_crate_file(&name, &version).ok() {
		Some(f) => f,
		None => panic!("Version {} of crate {} not mirrored", version, name),
	};
	let cargo_toml_extracted = extract_path_from_gz(&f,
		&format!("{}-{}/Cargo.toml", name, version));
	f.seek(SeekFrom::Start(0)).unwrap();

	let cargo_toml_file = if let Some(toml_file) = cargo_toml_extracted {
		toml_file
	} else {
		return None;
	};

	let info :CrateInfo = otry!(toml::from_slice(&cargo_toml_file));

	let readme_html = if let Some(filename) = info.package.readme {
		if let Some(c) = extract_path_from_gz(&f,
				&format!("{}-{}/{}", name, version, filename)) {
			if let Ok(s) = String::from_utf8(c) {
				let p = Parser::new(&s);
				let mut r = String::new();
				html::push_html(&mut r, p);
				Some(r)
			} else {
				None
			}
		} else {
			None
		}
	} else {
		None
	};
	let versions = crate_json.iter()
		.map(|v| v.version.clone())
		.collect::<Vec<_>>();
	let (v_start, v_limited) = if versions.len() > 5 {
		(versions.len() - 5, true)
	} else {
		(0, false)
	};
	let json_for_version = crate_json.iter()
		.filter(|v| v.version == version).next().unwrap();

	let dev_deps :Vec<Dependency> = json_for_version.dependencies.iter()
			.filter(|d| d.kind == DependencyKind::Dev)
			.map(|d| Dependency { name : d.name.clone(), req : d.req.clone() })
			.collect();

	let krate = Crate {
		name : name.clone(),
		version : version.to_string(),
		description : info.package.description,
		readme_html : readme_html,
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
		dependencies : json_for_version.dependencies.iter()
			.filter(|d| d.kind == DependencyKind::Normal)
			.map(|d| Dependency { name : d.name.clone(), req : d.req.clone() })
			.collect(),
		dev_dependencies : if dev_deps.len() > 0 {
			Some(dev_deps)
		} else {
			None
		},
	};
	data.insert("c".to_string(), to_json(&krate));
	Some(data)
}
