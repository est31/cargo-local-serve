use semver::Version;
use super::hash_ctx::{HashCtx, Digest};
use super::registry::registry::{CrateIndexJson, AllCratesJson};
use std::path::{Path, PathBuf};
use std::cell::RefCell;
use std::fs::File;
use std::io;
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

pub trait CrateSource {
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

impl CrateSource for FileTreeStorage {
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
