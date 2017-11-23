use std::{io, env};
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::fs::File;
use semver::VersionReq;
use serde_json::from_str;
use std::fmt;
use serde::de::{self, Deserialize, Deserializer, Visitor};
use failure::{Context, ResultExt};

use semver::Version;
use git2::{self, Repository};

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

fn obtain_crate_name_path(name :&str) -> String {
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

impl From<RegistryErrorKind> for Context<RegistryErrorKind> {
	fn from(kind :RegistryErrorKind) -> Self {
		Context::new(kind)
	}
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
			res.push((name.to_owned(), json));
			Ok(())
		}));
		Ok(res)
	}
	pub fn get_crate_file(&self, crate_name :&str, crate_version :&Version) ->
			io::Result<File> {
		let p = self.cache_path.join(format!("{}-{}.crate",
			crate_name, crate_version));
		Ok(try!(File::open(p)))
	}
}
