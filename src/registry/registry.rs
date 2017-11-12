use std::{io, env};
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::fs::{read_dir, File};
use semver::VersionReq;
use serde_json::from_str;

use semver::Version;

use super::Dependency;

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

// These crates have a bug in their json files:
// the dependencies don't contain a kind.
// https://github.com/rust-lang/crates.io/issues/1168
const IGNORED_NAMES :&[&'static str] = &[
	"unicode_names_macros", "tempan", "pipes", "pidfile",
	"plugin", "uchardet-sys", "uchardet", "julius", "openssl-sys",
	"opentype", "openssl2-sys", "leveldb", "tgff", "vorbisfile-sys",
	"vorbis-sys", "glium", "gl_generator", "glutin", "lazy",
	"lapack", "rope", "ogg-sys", "epsilonz", "matrix", "email",
	"hotspot", "miniz-sys", "redis", "resources_package",
	"rethinkdb", "basehangul", "free", "fractran_macros", "chrono",
	"docopt_macros", "date", "event", "event-emitter", "cssparser",
	"obj", "xsv", "zip", "mio", "url", "utp", "rusqlite",
	"rust-crypto", "capnp-rpc", "capnpc", "probability", "image",
	"git2", "fingertree", "diecast", "blas", "slow_primes", "gl",
	"enforce", "encoding-index-tradchinese", "encoding",
	"encoding-index-simpchinese", "encoding-index-japanese",
	"encoding-index-korean", "encoding-index-singlebyte",
	"bzip2-sys", "bzip2", "conduit-log-requests",
	"conduit-middleware", "conduit-utils",
	"conduit-conditional-get", "conduit-router", "conduit",
	"conduit-test", "conduit-static", "conduit-json-parser",
	"typemap", "libssh2-sys", "liblapack-sys", "libz-sys",
	"libgit2-sys", "libressl-pnacl-sys", "quickcheck_macros",
	"error", "tiled", "time", "civet", "ssh2", "stream",
	"strided", "string_telephone", "sfunc", "ordered-float",
	"flate2", "monad", "modifier", "tailrec", "tabwriter",
	"taskpool", "fftw3-sys", "grabbag"
];

pub fn obtain_all_crates_paths(index_root :&Path)
		-> io::Result<Vec<(String, PathBuf)>> {
	fn walk<F :FnMut(String, PathBuf)>(r :&Path,
			d :bool, f :&mut F) -> io::Result<()> {
		for e in try!(read_dir(r)) {
			let entry = try!(e);
			let path = entry.path();
			if let Ok(s) = entry.file_name().into_string() {
				if { let s :&str = &s; IGNORED_NAMES.contains(&s) } {
					continue;
				}
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
