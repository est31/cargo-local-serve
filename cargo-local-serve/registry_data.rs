use hbs::handlebars::to_json;
use serde_json::value::{Value, Map};
use toml;
use semver::Version as SvVersion;
use semver::VersionReq;
use urlencoded::QueryMap;

use all_crate_storage::registry::registry::{Dependency, Registry, DependencyKind};
use all_crate_storage::registry::statistics::CrateStats;
use all_crate_storage::crate_storage::CrateSource;
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

pub fn get_crate_data<C :CrateSource>(name :String, reg :&Registry, st :&mut C,
		version :Option<&str>) -> Option<Map<String, Value>> {

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
	let mut fh = match st.get_crate_handle_nv(name.to_owned(), version.clone()) {
		Some(f) => f,
		None => panic!("Version {} of crate {} not mirrored", version, name),
	};
	let cargo_toml_extracted = fh.get_file(
		&format!("{}-{}/Cargo.toml", name, version));

	let cargo_toml_file = if let Some(toml_file) = cargo_toml_extracted {
		toml_file
	} else {
		return None;
	};

	let info :CrateInfo = toml::from_slice(&cargo_toml_file).unwrap();

	let readme_html = if let Some(filename) = info.package.readme {
		if let Some(c) = fh.get_file(
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

pub fn get_versions_data(name :&str, reg :&Registry, refferer :Option<String>)
		-> Map<String, Value> {

	#[derive(Serialize, Debug)]
	struct Versions {
		name :String,
		refferer :Option<String>,
		versions_length :usize,
		versions :Vec<Version>,
	}

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

pub fn get_reverse_dependencies(name :&str,
		only_latest_versions :bool,
		stats :&CrateStats, refferer :Option<String>) -> Map<String, Value> {

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
		-> (Map<String, Value>, Option<String>) {

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

	let maybe_only_one = if results.results_length == 1 {
		let only_crate = results.results.iter().next().unwrap();
		Some(only_crate.name.clone())
	} else {
		None
	};

	(data, maybe_only_one)
}

pub enum CrateFileData {
	FileListing(Map<String, Value>),
	FileContent(Map<String, Value>),
}

pub fn get_crate_file_data<C :CrateSource>(st :&mut C,
	name :&str, version_str :&str, path :&[&str])
		-> CrateFileData {
	use std::str;
	use syntect_format::SyntectFormatter;

	let mut data = Map::new();

	// First step: find the path to the crate.
	let version = SvVersion::parse(version_str).unwrap();
	let mut fh = match st.get_crate_handle_nv(name.to_owned(), version.clone()) {
		Some(f) => f,
		None => panic!("Version {} of crate {} not mirrored", version, name),
	};
	let file_path_str = path.iter().fold(String::new(), |s, u| s + "/" + u);

	if file_path_str.len() <= 1 {

		#[derive(Serialize, Debug)]
		struct FileEntry {
			name :String,
		}

		#[derive(Serialize, Debug)]
		struct CrateFileListing {
			name :String,
			version :String,
			file_count :usize,
			files :Vec<FileEntry>,
		}

		let file_list = fh.get_file_list();

		let listing = CrateFileListing {
			name : name.to_owned(),
			version : version_str.to_owned(),
			file_count : file_list.len(),
			files : file_list.into_iter().map(|s| FileEntry {
				name : s,
			}).collect::<Vec<_>>(),
		};
		data.insert("c".to_string(), to_json(&listing));

		CrateFileData::FileListing(data)
	} else {
		#[derive(Serialize, Debug)]
		struct CrateFileContent {
			name :String,
			version :String,
			file_path :String,
			content_html :String,
		}
		let content_raw = fh.get_file(&file_path_str[1..])
			.expect("Path not found in crate file");
		let content_html = match str::from_utf8(&content_raw) {
			Ok(content_str) => {
				let extension = if file_path_str.contains(".") {
					file_path_str.split(".").last()
				} else {
					None
				};
				if extension == Some("md") {
					render_markdown(content_str)
				} else {
					let mut fmt = SyntectFormatter::new();
					if let Some(ext) = extension {
						fmt = fmt.extension(ext);
					}
					let html_unsanitized = fmt.highlight_snippet(content_str);
					// TODO sanitize using ammonia
					html_unsanitized
				}
			},
			Err(_) => {
				"(Not in UTF-8 format)".to_owned()
			},
		};
		let content = CrateFileContent {
			name : name.to_owned(),
			version : version_str.to_owned(),
			file_path : file_path_str,
			content_html
		};
		data.insert("c".to_string(), to_json(&content));
		CrateFileData::FileContent(data)
	}


}
