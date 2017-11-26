use hbs::handlebars::to_json;
use serde_json::value::{Value, Map};
use flate2::FlateReadExt;
use tar::Archive;
use std::io::{Read, Seek, SeekFrom};
use toml;
use semver::Version as SvVersion;
use semver::VersionReq;
use urlencoded::QueryMap;

use cargo_local_serve::registry::registry::{Dependency, Registry, DependencyKind};
use cargo_local_serve::registry::statistics::CrateStats;
use super::markdown_render::render_markdown;

#[derive(Serialize, Debug)]
pub struct Crate {
	name :String,
	version :String,
	homepage :Option<String>,
	documentation :String,
	repository :Option<String>,
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

#[allow(dead_code)]
pub fn winapi_crate_data() -> Map<String, Value> {
	let mut data = Map::new();

	let krate = Crate {
		name : "winapi".to_string(),
		version : "0.2.8".to_string(),
		homepage : None,
		documentation : "https://docs.rs/winapi".to_string(),
		repository : Some("https://github.com/retep998/winapi-rs".to_string()),
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
	repository :Option<String>,
	homepage :Option<String>,
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

pub fn get_crate_data(name :String, reg :&Registry, version :Option<&str>)
		-> Option<Map<String, Value>> {
	let mut data = Map::new();

	// First step: find the path to the crate.
	let crate_json = reg.get_crate_json(&name).unwrap();
	let version = if let Some(v) = version {
		SvVersion::parse(v).unwrap()
	} else {
		// Finds the latest version
		// TODO handle the case that there is no version
		// -- then the crate is not present!!!
		crate_json.iter().map(|v| &v.version).max().unwrap().clone()
	};
	let mut f = match reg.get_crate_file(&name, &version).ok() {
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
				Some(render_markdown(&s))
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
			.map(|d| d.to_crate_dep())
			.collect();

	let krate = Crate {
		name : name.clone(),
		version : version.to_string(),
		homepage : info.package.homepage,
		documentation : format!("https://docs.rs/{}/{}", name.clone(), version.to_string()),
		repository : info.package.repository,
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
			.map(|d| d.to_crate_dep())
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

#[derive(Serialize, Debug)]
struct Versions {
	name :String,
	refferer :Option<String>,
	versions_length :usize,
	versions :Vec<Version>,
}

pub fn get_versions_data(name :&str, reg :&Registry, refferer :Option<String>)
		-> Map<String, Value> {
	let mut data = Map::new();

	let crate_json = reg.get_crate_json(&name).unwrap();

	let version_list = crate_json.iter()
		.map(|jl| Version {
			v : format!("{}", jl.version),
			date : None,
		})
		.collect::<Vec<Version>>();

	let versions = Versions {
		name : name.to_string(),
		refferer,
		versions_length : version_list.len(),
		versions : version_list,
	};
	data.insert("c".to_string(), to_json(&versions));
	data
}

#[derive(Serialize, Debug)]
struct RevDep {
	name :String,
	req :VersionReq,
	version :SvVersion,
}

#[derive(Serialize, Debug)]
struct RevDependencies {
	name :String,
	refferer :Option<String>,
	rev_d_len :usize,
	rev_d :Vec<RevDep>,
}

pub fn get_reverse_dependencies(name :&str,
		only_latest_versions :bool,
		stats :&CrateStats, refferer :Option<String>) -> Map<String, Value> {
	let mut data = Map::new();

	// TODO don't use unwrap, and use "checked" getting below.
	// Give an error instead in both cases!
	let name_i = stats.crate_names_interner.get(name).unwrap();
	let mut rev_d_list = Vec::new();
	for (vreq, dlist) in stats.reverse_dependencies[&name_i].iter() {
		for &(rev_d_name, ref rev_d_version) in dlist.iter() {
			if only_latest_versions &&
					rev_d_version != &stats.latest_crate_versions[&rev_d_name] {
				// Ignore any non-latest version
				continue;
			}
			rev_d_list.push(RevDep {
				name : stats.crate_names_interner.resolve(rev_d_name)
					.unwrap().to_string(),
				req : vreq.clone(),
				version : rev_d_version.clone(),
			});
		}
	}

	let rev_deps = RevDependencies {
		name : name.to_string(),
		refferer,
		rev_d_len : rev_d_list.len(),
		rev_d : rev_d_list,
	};
	data.insert("c".to_string(), to_json(&rev_deps));
	data
}

pub fn get_index_data(stats :&CrateStats) -> Map<String, Value> {

	#[derive(Serialize, Debug)]
	struct CrateWithCount {
		name :String,
		count :usize,
	}

	#[derive(Serialize, Debug)]
	struct Index {
		direct_rev_deps :Vec<CrateWithCount>,
		transitive_rev_deps :Vec<CrateWithCount>,
		most_versions :Vec<CrateWithCount>,
	}

	let mut data = Map::new();

	let transitive_rev_deps = vec![]; // TODO populate

	let ddon = &stats.most_directly_depended_on;
	let mut direct_rev_deps = ddon[ddon.len().saturating_sub(10)..].iter()
		.map(|&(name, count)| CrateWithCount {
			name : stats.crate_names_interner.resolve(name)
				.unwrap().to_string(),
			count,
		})
		.collect::<Vec<_>>();
	direct_rev_deps.reverse();

	let most_v = &stats.most_versions;
	let mut most_versions = most_v[most_v.len().saturating_sub(10)..].iter()
		.map(|&(name, count)| CrateWithCount {
			name : stats.crate_names_interner.resolve(name)
				.unwrap().to_string(),
			count,
		})
		.collect::<Vec<_>>();
	most_versions.reverse();

	let index = Index {
		transitive_rev_deps,
		direct_rev_deps,
		most_versions,
	};
	data.insert("c".to_string(), to_json(&index));

	data
}

pub fn get_search_result_data(stats :&CrateStats, query_map :&QueryMap)
		-> Map<String, Value> {

	let search_term = (&query_map["q"][0]).clone(); // TODO add error handling

	let results = stats.crate_names_interner.iter_values()
		.filter(|s| s.contains(&search_term))
		.map(|s| SearchResult { name : s.to_owned() })
		.collect::<Vec<_>>();

	#[derive(Serialize, Debug)]
	struct SearchResult {
		name :String,
	}

	#[derive(Serialize, Debug)]
	struct SearchResults {
		search_term :String,
		results :Vec<SearchResult>,
		results_length :usize,
	}

	let mut data = Map::new();

	let results = SearchResults {
		search_term,
		results_length : results.len(),
		results,
	};
	data.insert("c".to_string(), to_json(&results));

	data
}
