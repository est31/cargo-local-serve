use semver::Version;
use super::hash_ctx::{HashCtx, Digest};
use super::registry::registry::{CrateIndexJson, AllCratesJson};
use flate2::read::GzDecoder;
use tar::Archive;
use std::path::{Path, PathBuf};
use std::cell::RefCell;
use std::fs::File;
use std::io::{self, Read};
use registry::registry::obtain_crate_name_path;

#[derive(Clone, PartialEq, Eq)]
pub struct CrateSpec {
	pub name :String,
	pub version :Version,
}

impl CrateSpec {
	pub fn file_name(&self) -> String {
		format!("{}-{}.crate", self.name, self.version)
	}
}

pub struct CrateHandle<'a, S :CrateSource + 'a, C :CrateFileHandle<S>> {
	pub source :&'a mut S,
	pub crate_file_handle :C,
}

impl<'a, S :CrateSource + 'a, C :CrateFileHandle<S>> CrateHandle<'a, S, C> {
	pub fn get_file_list(&mut self) -> Vec<String> {
		self.crate_file_handle.get_file_list(&mut self.source)
	}
	pub fn get_file(&mut self, path :&str) -> Option<Vec<u8>> {
		self.crate_file_handle.get_file(&mut self.source, path)
	}
}

pub trait CrateFileHandle<S :CrateSource> {
	fn get_file_list(&self, source :&mut S) -> Vec<String>;
	fn get_file(&self, source :&mut S, path :&str) -> Option<Vec<u8>>;
}

pub trait CrateSource :Sized {
	type CrateHandle :CrateFileHandle<Self>;
	fn get_crate_handle_nv(&mut self,
			name :String, version :Version) -> Option<CrateHandle<Self, Self::CrateHandle>>;
	fn get_crate_nv(&mut self, name :String, version :Version) -> Option<Vec<u8>> {
		self.get_crate(&CrateSpec {
			name,
			version,
		})
	}
	fn get_crate(&mut self, spec :&CrateSpec) -> Option<Vec<u8>>;
}

pub trait CrateStorage {
	fn store_parallel_iter<I :Iterator<Item = (CrateSpec, Vec<u8>, Digest)>>(
			&mut self, thread_count :u16, crate_iter :I);

	fn fill_crate_storage_from_source<S :CrateSource>(&mut self,
			thread_count :u16, acj :&AllCratesJson, source :&mut S,
			progress_callback :fn(&str, &CrateIndexJson)) {
		// Iterators are cool they told me.
		// Iterators are idiomatic they told me.
		// THEN WHY THE FUCK DO I NEED THIS REFCELL CRAP?!?!?!
		// https://stackoverflow.com/a/28521985
		let source_cell = RefCell::new(source);
		let crate_iter = acj.iter()
			.flat_map(|&(ref name, ref versions)| {
				let name = name.clone();
				let source_cell = &source_cell;
				versions.iter().filter_map(move |v| {
					let name = name.clone();
					let mut source = source_cell.borrow_mut();
					progress_callback(&name, &v);

					let spec = CrateSpec {
						name : name.to_owned(),
						version : v.version.clone(),
					};
					let crate_file_buf = match source.get_crate(&spec) {
						Some(cfb) => cfb,
						None => return None,
					};

					let mut hctx = HashCtx::new();
					io::copy(&mut crate_file_buf.as_slice(), &mut hctx).unwrap();
					let d = hctx.finish_and_get_digest();
					Some((spec, crate_file_buf, d))
				})
			});
		self.store_parallel_iter(thread_count, crate_iter);
	}
}

pub struct FileTreeStorage {
	storage_base :PathBuf,
}

impl FileTreeStorage {
	pub fn new(storage_base :&Path) -> Self {
		FileTreeStorage {
			storage_base : storage_base.to_path_buf(),
		}
	}
}


pub struct BlobCrateHandle {
	content :Vec<u8>,
}

impl BlobCrateHandle {
	pub fn new(content :Vec<u8>) -> Self {
		BlobCrateHandle {
			content
		}
	}
}

impl<S :CrateSource> CrateFileHandle<S> for BlobCrateHandle {
	fn get_file_list(&self, source :&mut S) -> Vec<String> {
		let f = self.content.as_slice();
		let mut l = Vec::new();
		let decoded = GzDecoder::new(f);
		let mut archive = Archive::new(decoded);
		for entry in archive.entries().unwrap() {
			let mut entry = entry.unwrap();
			let path = entry.path().unwrap();
			let s :String = path.to_str().unwrap().to_owned();
			l.push(s);
		}
		l
	}
	fn get_file(&self, _ :&mut S, path :&str) -> Option<Vec<u8>> {
		extract_path_from_gz(self.content.as_slice(), path)
	}
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

fn extract_path_from_gz<T :Read>(r :T,
		path_ex :&str) -> Option<Vec<u8>> {
	let decoded = GzDecoder::new(r);
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


impl CrateSource for FileTreeStorage {
	type CrateHandle = BlobCrateHandle;
	fn get_crate_handle_nv(&mut self,
			name :String, version :Version) -> Option<CrateHandle<Self, Self::CrateHandle>> {
		if let Some(content) = self.get_crate_nv(name, version) {
			Some(CrateHandle {
				source : self,
				crate_file_handle : BlobCrateHandle::new(content),
			})
		} else {
			None
		}
	}
	fn get_crate(&mut self, spec :&CrateSpec) -> Option<Vec<u8>> {
		let crate_file_path = self.storage_base
			.join(obtain_crate_name_path(&spec.name))
			.join(spec.file_name());
		let mut f = match File::open(&crate_file_path) {
			Ok(f) => f,
			Err(_) => {
				return None;
			},
		};
		let mut file_buf = Vec::new();
		io::copy(&mut f, &mut file_buf).unwrap();
		Some(file_buf)
	}
}

pub struct CacheStorage {
	storage_base :PathBuf,
}

impl CacheStorage {
	pub fn new(storage_base :&Path) -> Self {
		CacheStorage {
			storage_base : storage_base.to_path_buf(),
		}
	}
}

impl CrateSource for CacheStorage {
	type CrateHandle = BlobCrateHandle;
	fn get_crate_handle_nv(&mut self,
			name :String, version :Version) -> Option<CrateHandle<Self, Self::CrateHandle>> {
		if let Some(content) = self.get_crate_nv(name, version) {
			Some(CrateHandle {
				source : self,
				crate_file_handle : BlobCrateHandle::new(content),
			})
		} else {
			None
		}
	}
	fn get_crate(&mut self, spec :&CrateSpec) -> Option<Vec<u8>> {
		let crate_file_path = self.storage_base
			.join(spec.file_name());
		let mut f = match File::open(&crate_file_path) {
			Ok(f) => f,
			Err(_) => {
				return None;
			},
		};
		let mut file_buf = Vec::new();
		io::copy(&mut f, &mut file_buf).unwrap();
		Some(file_buf)
	}
}
