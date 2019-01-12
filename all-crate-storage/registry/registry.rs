use std::{io, env};
use std::io::BufRead;
use std::path::{Path, PathBuf};
use semver::VersionReq;
use serde_json::from_str;
use serde::de::{Deserialize, Deserializer};
use failure::{Context, ResultExt};

use semver::Version;
use git2::{self, Repository};
use super::super::crate_storage::CacheStorage;

#[derive(Serialize, Debug)]
pub struct Dependency {
	name :String,
	req :String,
	optional :bool,
}

#[derive(Deserialize, PartialEq, Eq, Hash, Debug, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum DependencyKind {
	Normal,
	Build,
	Dev,
}

// custom function needed due to https://github.com/serde-rs/serde/issues/1098
// as default + rename_all = "lowercase" does not cover the kind: null case :/
fn nullable_dep_kind<'de, D :Deserializer<'de>>(deserializer :D)
		-> Result<DependencyKind, D::Error> {
	let opt = try!(Option::deserialize(deserializer));
	Ok(opt.unwrap_or(DependencyKind::Normal))
}

fn normal_dep_kind() -> DependencyKind {
	DependencyKind::Normal
}

// TODO tests for dependency kind set to null or non existent.

#[derive(Deserialize, Clone)]
pub struct CrateDepJson {
	pub name :String,
	pub features :Vec<String>,
	pub default_features :bool,
	pub target :Option<String>,
	pub req :VersionReq,
	pub optional :bool,
	// We need to set a default as kind may not always be != null,
	// or it may not be existent.
	// https://github.com/rust-lang/crates.io/issues/1168
	#[serde(default = "normal_dep_kind", deserialize_with = "nullable_dep_kind")]
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

#[derive(Deserialize, Clone)]
pub struct CrateIndexJson {
	pub name :String,
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

pub fn obtain_crate_name_path(name :&str) -> String {
	match name.len() {
		1 => format!("1/{}", name),
		2 => format!("2/{}", name),
		3 => format!("3/{}/{}", &name[..1], name),
		_ => format!("{}/{}/{}", &name[..2], &name[2..4], name),
	}
}

fn buf_to_index_json(buf :&[u8]) -> io::Result<Vec<CrateIndexJson>> {
	let mut r = Vec::new();
	for l in buf.lines() {
		let l = try!(l);
		r.push(try!(from_str(&l)));
	}
	Ok(r)
}

pub type AllCratesJson = Vec<(String, Vec<CrateIndexJson>)>;

#[derive(Copy, Clone, Eq, PartialEq, Debug, Fail)]
pub enum RegistryErrorKind {
	#[fail(display = "Opening Registry failed")]
	RegOpen,
	#[fail(display = "Index reading failed")]
	IndexRepoReading,
	#[fail(display = "Index JSON reading failed")]
	IndexJsonReading,
	#[fail(display = "Index JSON file not found")]
	IndexJsonMissing,
}

pub type RegistryError = Context<RegistryErrorKind>;

fn get_repo_head_tree<'a>(repo :&'a Repository)
		-> Result<git2::Tree<'a>, git2::Error> {
	let head_id = try!(repo.refname_to_id("refs/remotes/origin/master"));
	let head_commit = try!(repo.find_commit(head_id));
	let head_tree = try!(head_commit.tree());
	Ok(head_tree)
}

impl Registry {
	pub fn from_name(name :&str) -> Result<Self, env::VarError> {
		// The name is the name + hash pair.
		// For crates.io it is "github.com-1ecc6299db9ec823"
		let home = try!(env::var("HOME"));
		let base_path = Path::new(&home).join(".cargo/registry/");
		let cache_path = base_path.join("cache").join(name);
		//let cache_path = env::current_dir().unwrap().join("crate-archives");
		let index_path = base_path.join("index").join(name);
		Ok(Registry {
			cache_path,
			index_path,
		})
	}
	pub fn get_crate_json(&self, crate_name :&str)
			-> Result<Vec<CrateIndexJson>, RegistryError> {
		use self::RegistryErrorKind::*;

		let repo = try!(git2::Repository::open(&self.index_path).context(RegOpen));
		let head_tree = try!(get_repo_head_tree(&repo).context(IndexRepoReading));

		let path_str = obtain_crate_name_path(crate_name);
		let entry = try!(head_tree.get_path(&Path::new(&path_str))
			.with_context(|e :&git2::Error| {
				if e.code() == git2::ErrorCode::NotFound {
					IndexJsonMissing
				} else {
					IndexJsonReading
				}
			}));
		let obj = try!(entry.to_object(&repo).context(IndexRepoReading));
		let bytes :&[u8] = match obj.as_blob() {
			Some(b) => b.content(),
			None => try!(Err(IndexRepoReading)),
		};
		let json = try!(buf_to_index_json(bytes).context(IndexJsonReading));
		Ok(json)
	}
	pub fn get_all_crates_json(&self) ->
			Result<AllCratesJson, RegistryError> {
		use self::RegistryErrorKind::*;

		let repo = try!(git2::Repository::open(&self.index_path).context(RegOpen));
		let head_tree = try!(get_repo_head_tree(&repo).context(IndexRepoReading));

		fn walk<F :FnMut(&str, &[u8]) -> Result<(), RegistryError>>(
				t :&git2::Tree, repo :&git2::Repository,
				d :bool, f :&mut F) -> Result<(), RegistryError> {
			for entry in t.iter() {
				let entry_obj = try!(entry.to_object(&repo)
					.context(IndexRepoReading));
				if let Some(tree) = entry_obj.as_tree() {
					try!(walk(tree, repo, false, f));
				} else if let Some(blob) = entry_obj.as_blob() {
					if !d {
						let name = match entry.name() {
							Some(v) => v,
							None => try!(Err(IndexRepoReading)),
						};
						try!(f(name, blob.content()));
					}
				}
			}
			Ok(())
		}
		let mut res = Vec::new();
		try!(walk(&head_tree, &repo, true, &mut |name, blob| {
			let json = try!(buf_to_index_json(blob).context(IndexJsonReading));
			let name = if let Some(v) = json.iter().next() {
				// This is important as the file name is always in lowercase,
				// but the actual name of the crate may have mixed casing.
				v.name.to_owned()
			} else {
				name.to_owned()
			};
			res.push((name, json));
			Ok(())
		}));
		Ok(res)
	}
	pub fn get_cache_storage(&self) -> CacheStorage {
		CacheStorage::new(&self.cache_path)
	}
}
