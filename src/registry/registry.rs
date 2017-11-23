use std::{io, env};
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::fs::{read_dir, File};
use semver::VersionReq;
use serde_json::from_str;
use std::fmt;
use serde::de::{self, Deserialize, Deserializer, Visitor};

use semver::Version;

use super::Dependency;

#[derive(PartialEq, Eq, Hash, Debug)]
pub enum DependencyKind {
	Normal,
	Build,
	Dev,
}

// custom impl needed due to https://github.com/serde-rs/serde/issues/1098
// as default + rename_all = "lowercase" does not cover the kind: null case :/
impl<'de> Deserialize<'de> for DependencyKind {
	fn deserialize<D :Deserializer<'de>>(deserializer: D)
			-> Result<Self, D::Error> {
		struct DkVisitor;

		impl<'de> Visitor<'de> for DkVisitor {
			type Value = DependencyKind;

			fn expecting(&self, formatter :&mut fmt::Formatter) -> fmt::Result {
				formatter.write_str("`normal` or `build` or `dev`")
			}

			fn visit_none<E :de::Error>(self)
					-> Result<DependencyKind, E> {
				// We need to set a default as kind may not always be != null,
				// or it may not be existent.
				// https://github.com/rust-lang/crates.io/issues/1168
				Ok(DependencyKind::Normal)
			}
			fn visit_some<D :Deserializer<'de>>(self, d :D)
					-> Result<DependencyKind, D::Error> {
				d.deserialize_any(DkVisitor)
			}
			fn visit_str<E :de::Error>(self, value :&str)
					-> Result<DependencyKind, E> {
				match value {
					"normal" => Ok(DependencyKind::Normal),
					"build" => Ok(DependencyKind::Build),
					"dev" => Ok(DependencyKind::Dev),
					_ => Err(de::Error::unknown_field(value,
						&["normal", "build", "dev"])),
				}
			}
		}
		deserializer.deserialize_option(DkVisitor)
	}
}

// TODO tests for dependency kind set to null or non existent.

#[derive(Deserialize)]
pub struct CrateDepJson {
	pub name :String,
	pub features :Vec<String>,
	pub default_features :bool,
	pub target :Option<String>,
	pub req :VersionReq,
	pub optional :bool,
	pub kind :DependencyKind,
}

impl CrateDepJson {
	pub fn to_crate_dep(&self) -> Dependency {
		Dependency {
			name : self.name.clone(),
			req : self.req.to_string(),
			optional : self.optional,
		}
	}
}

#[derive(Deserialize)]
pub struct CrateIndexJson {
	#[serde(rename = "vers")]
	pub version :Version,
	#[serde(rename = "deps")]
	pub dependencies :Vec<CrateDepJson>,
	#[serde(rename = "cksum")]
	pub checksum :String,
	// TODO features
	pub yanked :bool,
}

pub struct Registry {
	cache_path :PathBuf,
	index_path :PathBuf,
}

fn obtain_index_path(index_root :&Path, name :&str) -> io::Result<Option<PathBuf>> {
	fn obtain_index_path_inner(r :&Path, n :&str, n_orig :&str) ->
			io::Result<Option<PathBuf>> {
		for e in try!(read_dir(r)) {
			let entry = try!(e);
			let path = entry.path();
			if let Ok(s) = entry.file_name().into_string() {
				if path.is_dir() {
					if n.starts_with(&s) {
						return obtain_index_path_inner(&path,
							&n[s.len()..], n_orig);
					}
				} else {
					if n_orig == s {
						return Ok(Some(path));
					}
				}
			}
		}
		return Ok(None);
	}
	if name.len() < 4 {
		return obtain_index_path_inner(&index_root.join(format!("{}", name.len())),
			name, name);
	}
	obtain_index_path_inner(index_root, name, name)
}

fn path_to_index_json(path :&Path) -> io::Result<Vec<CrateIndexJson>> {
	let f = try!(File::open(path));
	let br = io::BufReader::new(f);
	let mut r = Vec::new();
	for l in br.lines() {
		let l = try!(l);
		r.push(try!(from_str(&l)));
	}
	Ok(r)
}

pub type AllCratesJson = Vec<(String, Vec<CrateIndexJson>)>;

impl Registry {
	pub fn from_name(name :&str) -> Result<Self, env::VarError> {
		// The name is the name + hash pair.
		// For crates.io it is "github.com-1ecc6299db9ec823"
		let home = try!(env::var("HOME"));
		let base_path = Path::new(&home).join(".cargo/registry/");
		let cache_path = base_path.join("cache").join(name);
		let index_path = base_path.join("index").join(name);
		Ok(Registry {
			cache_path,
			index_path,
		})
	}
	pub fn get_crate_json(&self, crate_name :&str) ->
			io::Result<Vec<CrateIndexJson>> {
		let json_path = try!(obtain_index_path(&self.index_path, crate_name));
		let json_path = if let Some(p) = json_path {
			p
		} else {
			return Ok(vec![]);
		};
		let r = try!(path_to_index_json(&json_path));
		Ok(r)
	}
	pub fn get_all_crates_json(&self) ->
			io::Result<AllCratesJson> {
		let paths = try!(obtain_all_crates_paths(&self.index_path));
		let mut r = Vec::with_capacity(paths.len());
		for (crate_name, path) in paths {

			let json = try!(path_to_index_json(&path));
			/*let json = match path_to_index_json(&path) {
				Ok(v) => v,
				Err(_) => { println!("{:?}", &path); continue;},
			};*/
			r.push((crate_name, json));
		}
		Ok(r)
	}
	pub fn get_crate_file(&self, crate_name :&str, crate_version :&Version) ->
			io::Result<File> {
		let p = self.cache_path.join(format!("{}-{}.crate",
			crate_name, crate_version));
		Ok(try!(File::open(p)))
	}
}

pub fn obtain_all_crates_paths(index_root :&Path)
		-> io::Result<Vec<(String, PathBuf)>> {
	fn walk<F :FnMut(String, PathBuf)>(r :&Path,
			d :bool, f :&mut F) -> io::Result<()> {
		for e in try!(read_dir(r)) {
			let entry = try!(e);
			let path = entry.path();
			if let Ok(s) = entry.file_name().into_string() {
				if s == ".git" && !d {
					// We don't want to traverse into
					// the .git directory.
					continue;
				}
				if path.is_dir() {
					try!(walk(&path, true, f));
				} else {
					// This check is to prevent files on the first level
					// to be added.
					// There is only one such file: it is config.json.
					// We don't want to output it!
					if d {
						f(s, path);
					}
				}
			}
		}
		Ok(())
	}
	let mut res = Vec::new();
	try!(walk(index_root, false, &mut |s, p| {
		res.push((s, p));
	}));
	Ok(res)
}
