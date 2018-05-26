use semver::Version;
use super::hash_ctx::{HashCtx, Digest};
use super::registry::registry::{CrateIndexJson, AllCratesJson};
use super::blob_crate_storage::{BlobCrateStorage, StorageFileHandle};
use flate2::read::GzDecoder;
use tar::Archive;
use std::path::{Path, PathBuf};
use std::cell::RefCell;
use std::fs::File;
use std::io::{self, Read, Seek};
use std::ops::Deref;
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

impl<S :CrateSource> CrateFileHandle<S> for Box<CrateFileHandle<S>> {
	fn get_file_list(&self, source :&mut S) -> Vec<String> {
		<Box<_> as Deref>::deref(self).get_file_list(source)
	}
	fn get_file(&self, source :&mut S, path :&str) -> Option<Vec<u8>> {
		<Box<_> as Deref>::deref(self).get_file(source, path)
	}
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

pub enum DynCrateSource<S :Read + Seek> {
	FileTreeStorage(FileTreeStorage),
	CacheStorage(CacheStorage),
	BlobCrateStorage(BlobCrateStorage<S>),
	OverlayCrateSource(Box<OverlayCrateSource<DynCrateSource<S>, DynCrateSource<S>>>),
}

pub enum DynCrateHandle<S :Read + Seek> {
	BlobCrateHandle(BlobCrateHandle),
	StorageFileHandle(StorageFileHandle),
	OverlayCrateHandle(Box<OverlayCrateHandle<DynCrateSource<S>, DynCrateSource<S>>>),
}

impl<S :Read + Seek> DynCrateHandle<S> {
	fn blob(&self) -> Option<&BlobCrateHandle> {
		match self {
			&DynCrateHandle::BlobCrateHandle(ref h) => Some(h),
			_ => None,
		}
	}
	fn storage(&self) -> Option<&StorageFileHandle> {
		match self {
			&DynCrateHandle::StorageFileHandle(ref h) => Some(h),
			_ => None,
		}
	}
	fn overlay(&self) -> Option<&OverlayCrateHandle<DynCrateSource<S>, DynCrateSource<S>>> {
		match self {
			&DynCrateHandle::OverlayCrateHandle(ref h) => Some(h),
			_ => None,
		}
	}
}

impl<S :Read + Seek> CrateSource for DynCrateSource<S> {
	type CrateHandle = DynCrateHandle<S>;
	fn get_crate_handle_nv(&mut self,
			name :String, version :Version) -> Option<CrateHandle<Self, Self::CrateHandle>> {
		let ch = match self {
			&mut DynCrateSource::FileTreeStorage(ref mut s) => {
				s.get_crate_handle_nv(name, version)
					.map(|h| DynCrateHandle::BlobCrateHandle(h.crate_file_handle))
			},
			&mut DynCrateSource::CacheStorage(ref mut s) => {
				s.get_crate_handle_nv(name, version)
					.map(|h| DynCrateHandle::BlobCrateHandle(h.crate_file_handle))
			},
			&mut DynCrateSource::BlobCrateStorage(ref mut s) => {
				s.get_crate_handle_nv(name, version)
					.map(|h| DynCrateHandle::StorageFileHandle(h.crate_file_handle))
			},
			&mut DynCrateSource::OverlayCrateSource(ref mut s) => {
				s.get_crate_handle_nv(name, version)
					.map(|h| DynCrateHandle::OverlayCrateHandle(Box::new(h.crate_file_handle)))
			},
		};
		if let Some(ch) = ch {
			Some(CrateHandle {
				source : self,
				crate_file_handle : ch,
			})
		} else {
			None
		}
	}
	fn get_crate(&mut self, spec :&CrateSpec) -> Option<Vec<u8>> {
		match self {
			&mut DynCrateSource::FileTreeStorage(ref mut s) => {
				s.get_crate(spec)
			},
			&mut DynCrateSource::CacheStorage(ref mut s) => {
				s.get_crate(spec)
			},
			&mut DynCrateSource::BlobCrateStorage(ref mut s) => {
				s.get_crate(spec)
			},
			&mut DynCrateSource::OverlayCrateSource(ref mut s) => {
				s.get_crate(spec)
			},
		}
	}
}

impl<S :Read + Seek> CrateFileHandle<DynCrateSource<S>> for DynCrateHandle<S> {
	fn get_file_list(&self, source :&mut DynCrateSource<S>) -> Vec<String> {
		match source {
			&mut DynCrateSource::FileTreeStorage(ref mut s) => {
				self.blob().unwrap().get_file_list(s)
			},
			&mut DynCrateSource::CacheStorage(ref mut s) => {
				self.blob().unwrap().get_file_list(s)
			},
			&mut DynCrateSource::BlobCrateStorage(ref mut s) => {
				self.storage().unwrap().get_file_list(s)
			},
			&mut DynCrateSource::OverlayCrateSource(ref mut s) => {
				self.overlay().unwrap().get_file_list(s)
			},
		}
	}
	fn get_file(&self, source :&mut DynCrateSource<S>,
			path :&str) -> Option<Vec<u8>> {
		match source {
			&mut DynCrateSource::FileTreeStorage(ref mut s) => {
				self.blob().unwrap().get_file(s, path)
			},
			&mut DynCrateSource::CacheStorage(ref mut s) => {
				self.blob().unwrap().get_file(s, path)
			},
			&mut DynCrateSource::BlobCrateStorage(ref mut s) => {
				self.storage().unwrap().get_file(s, path)
			},
			&mut DynCrateSource::OverlayCrateSource(ref mut s) => {
				self.overlay().unwrap().get_file(s, path)
			},
		}
	}
}

pub struct OverlayCrateSource<S :CrateSource, T :CrateSource>(S, T);

impl<S :CrateSource, T :CrateSource> OverlayCrateSource<S, T> {
	pub fn new(default :S, fallback :T) -> Self {
		OverlayCrateSource(default, fallback)
	}
	fn get_overlay_crate_handle_nv(&mut self,
			name :String, version :Version) -> Option<OverlayCrateHandle<S, T>> {
		if let Some(v) = self.0.get_crate_handle_nv(name.clone(), version.clone()) {
			return Some(OverlayCrateHandle::DefaultFound(v.crate_file_handle));
		}
		if let Some(v) = self.1.get_crate_handle_nv(name, version) {
			return Some(OverlayCrateHandle::FallbackFound(v.crate_file_handle));
		}
		return None;
	}
}

impl<S :CrateSource, T :CrateSource> CrateSource for OverlayCrateSource<S, T> {
	type CrateHandle = OverlayCrateHandle<S, T>;
	fn get_crate_handle_nv(&mut self,
			name :String, version :Version) -> Option<CrateHandle<Self, Self::CrateHandle>> {
		if let Some(ch) = self.get_overlay_crate_handle_nv(name, version) {
			Some(CrateHandle {
				source : self,
				crate_file_handle : ch,
			})
		} else {
			None
		}
	}
	fn get_crate(&mut self, spec :&CrateSpec) -> Option<Vec<u8>> {
		if let Some(v) = self.0.get_crate(spec) {
			return Some(v);
		}
		return self.1.get_crate(spec);
	}
}

pub enum OverlayCrateHandle<D :CrateSource, F :CrateSource> {
	DefaultFound(D::CrateHandle),
	FallbackFound(F::CrateHandle),
}

impl<S :CrateSource, T: CrateSource> CrateFileHandle<OverlayCrateSource<S, T>> for OverlayCrateHandle<S, T> {
	fn get_file_list(&self, source :&mut OverlayCrateSource<S, T>) -> Vec<String> {
		match self {
			&OverlayCrateHandle::DefaultFound(ref s) => {
				s.get_file_list(&mut source.0)
			},
			&OverlayCrateHandle::FallbackFound(ref s) => {
				s.get_file_list(&mut source.1)
			},
		}
	}
	fn get_file(&self, source :&mut OverlayCrateSource<S, T>,
			path :&str) -> Option<Vec<u8>> {
		match self {
			&OverlayCrateHandle::DefaultFound(ref s) => {
				s.get_file(&mut source.0, path)
			},
			&OverlayCrateHandle::FallbackFound(ref s) => {
				s.get_file(&mut source.1, path)
			},
		}
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
	pub fn map_all_files<F :FnMut(Option<String>, Option<Vec<u8>>)>(&self, mut f :F) {
		let r = self.content.as_slice();
		let decoded = GzDecoder::new(r);
		let mut archive = Archive::new(decoded);
		for entry in archive.entries().unwrap() {
			let mut entry = if let Ok(entry) = entry {
				entry
			} else {
				continue;
			};
			let path :Option<String> = entry.path().ok()
				.and_then(|s| if let Some(s) = s.to_str() {
					Some(s.to_owned())
				} else {
					None
				});
			let mut v = Vec::new();
			let v = if entry.read_to_end(&mut v).is_ok() {
				Some(v)
			} else {
				None
			};
			f(path, v);
		}
	}
}

impl<S :CrateSource> CrateFileHandle<S> for BlobCrateHandle {
	fn get_file_list(&self, _source :&mut S) -> Vec<String> {
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
