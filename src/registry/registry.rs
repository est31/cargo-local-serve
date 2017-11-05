use std::{io, env};
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::fs::{read_dir, File};
use serde_json::from_str;

use semver::Version;

#[derive(Deserialize, PartialEq, Eq, Hash, Debug)]
pub enum DependencyKind {
	#[serde(rename = "normal")]
	Normal,
	#[serde(rename = "build")]
	Build,
	#[serde(rename = "dev")]
	Dev,
}

#[derive(Deserialize)]
pub struct CrateDepJson {
	pub name :String,
	pub req :String,
	pub kind :DependencyKind,
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
		for e in try!(read_dir(index_root.join(format!("{}", name.len())))) {
			let entry = try!(e);
			if Ok(name.to_owned())  == entry.file_name().into_string() {
				return Ok(Some(entry.path()));
			}
		}
		return Ok(None);
	}
	obtain_index_path_inner(index_root, name, name)
}

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
		let f = try!(File::open(json_path));
		let br = io::BufReader::new(f);
		let mut r = Vec::new();
		for l in br.lines() {
			let l = try!(l);
			r.push(try!(from_str(&l)));
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
