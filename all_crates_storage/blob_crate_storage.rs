use super::blob_storage::BlobStorage;
use super::hash_ctx::{HashCtx, Digest};
use super::reconstruction::{CrateContentBlobs, CrateRecMetadata,
	CrateRecMetaWithBlobs, hdr_from_ptr};
use super::crate_storage::{CrateStorage, CrateSpec, CrateSource,
	CrateHandle, CrateFileHandle};
use super::multi_blob::MultiBlob;
use super::diff::Diff;

use semver::Version;
use flate2::{Compression, GzBuilder};
use flate2::read::GzDecoder;
use std::io::{self, Read, Seek, Write, Result as IoResult};
use std::collections::HashSet;

pub struct BlobCrateStorage<S :Read + Seek> {
	pub(crate) b :BlobStorage<S>,
}

macro_rules! optry {
	($e:expr) => {
		match $e {
			Some(d) => d,
			None => return None,
		}
	};
}

macro_rules! decompress {
	($e:expr) => {{
		let mut gz_dec = GzDecoder::new($e.as_slice());
		let mut r = Vec::new();
		io::copy(&mut gz_dec, &mut r).unwrap();
		r
	}}
}

impl<S :Read + Seek> BlobCrateStorage<S> {
	pub fn empty(storage :S) -> Self {
		BlobCrateStorage {
			b : BlobStorage::empty(storage),
		}
	}
	pub fn new(storage :S) -> IoResult<Self> {
		Ok(BlobCrateStorage {
			b : try!(BlobStorage::new(storage)),
		})
	}
	pub fn load(storage :S) -> IoResult<Self> {
		Ok(BlobCrateStorage {
			b : try!(BlobStorage::load(storage)),
		})
	}
	pub(crate) fn get_crate_rec_meta(&mut self, s :&CrateSpec) -> Option<CrateRecMetadata> {
		let meta_d = optry!(self.b.name_index.get(&s.file_name())).clone();

		let cmeta = optry!(optry!(self.b.get(&meta_d).ok()));
		let dmeta = decompress!(cmeta);
		let meta = optry!(CrateRecMetadata::deserialize(dmeta.as_slice()).ok());
		Some(meta)
	}
}

impl<S :Read + Seek + Write> BlobCrateStorage<S> {
	pub fn store(&mut self) -> io::Result<()> {
		try!(self.b.write_header_and_index());
		Ok(())
	}
}

impl<S :Read + Seek + Write> CrateStorage for BlobCrateStorage<S> {
	fn store_parallel_iter<I :Iterator<Item = (CrateSpec, Vec<u8>, Digest)>>(
			&mut self, thread_count :u16, mut crate_iter :I) {
		use std::sync::mpsc::{sync_channel, TrySendError};
		use multiqueue::mpmc_queue;
		use std::time::Duration;
		use std::thread;

		let (bt_tx, bt_rx) = sync_channel(10);
		let (pt_tx, pt_rx) = mpmc_queue(10);
		for _ in 0 .. thread_count {
			let bt_tx = bt_tx.clone();
			let pt_rx = pt_rx.clone();
			thread::spawn(move || {
				while let Ok(task) = pt_rx.recv() {
					handle_parallel_task(task, |bt| bt_tx.send(bt).unwrap());
				}
			});
		}
		drop(bt_tx);
		pt_rx.unsubscribe();
		let mut par_task_backlog = Vec::new();
		let mut blobs_to_store = HashSet::new();
		loop {
			let mut done_something = false;
			if let Ok(task) = bt_rx.recv_timeout(Duration::new(0, 50_000)) {
				handle_blocking_task(task, &mut self.b,
					&mut blobs_to_store, |tsk| par_task_backlog.push(tsk));
				done_something = true;
			}
			if par_task_backlog.is_empty() {
				for _ in 0 .. 10 {
					if let Some((sp, b, d)) = crate_iter.next() {
						let name = sp.file_name();
						par_task_backlog.push(ParallelTask::ObtainCrateContentBlobs(name, b, d));
						done_something = true;
					}
				}
			}
			loop {
				let mut removed_something = false;
				if let Some(t) = par_task_backlog.pop() {
					if let Err(e) = pt_tx.try_send(t) {
						let t = match e {
							TrySendError::Full(t) => t,
							TrySendError::Disconnected(t) => t,
						};
						par_task_backlog.push(t);
					} else {
						removed_something = true;
						done_something = true;
					}
				}
				if !removed_something {
					break;
				}
			}
			if !done_something && par_task_backlog.is_empty() {
				break;
			}
		}
	}
}

pub struct StorageFileHandle {
	meta :CrateRecMetadata,
}

impl<S :Read + Seek> CrateFileHandle<BlobCrateStorage<S>> for StorageFileHandle {
	fn get_file_list(&self, _source :&mut BlobCrateStorage<S>) -> Vec<String> {
		self.meta.get_file_list()
	}
	fn get_file(&self, source :&mut BlobCrateStorage<S>,
			path :&str) -> Option<Vec<u8>> {
		for &(ref hdr, ref d) in self.meta.entry_metadata.iter() {
			let hdr = hdr_from_ptr(hdr);
			let p = hdr.path().unwrap();
			let s = p.to_str().unwrap();
			if s != path {
				continue;
			}
			let blob = optry!(optry!(source.b.get(d).ok()));
			let decompressed = decompress!(blob);
			return Some(decompressed);
		}
		None
	}
}

impl<S :Read + Seek> CrateSource for BlobCrateStorage<S> {

	type CrateHandle = StorageFileHandle;
	fn get_crate_handle_nv(&mut self,
			name :String, version :Version) -> Option<CrateHandle<Self, Self::CrateHandle>> {
		let s = CrateSpec {
			name,
			version,
		};
		let meta = optry!(self.get_crate_rec_meta(&s));

		Some(CrateHandle {
			source : self,
			crate_file_handle : StorageFileHandle {
				meta
			},
		})
	}
	fn get_crate(&mut self, s :&CrateSpec) -> Option<Vec<u8>> {
		let meta = optry!(self.get_crate_rec_meta(s));
		let mut blobs = Vec::with_capacity(meta.entry_metadata.len());
		for &(ref _hdr, ref d) in meta.entry_metadata.iter() {
			let blob = optry!(optry!(self.b.get(d).ok()));
			let decompressed = decompress!(blob);
			blobs.push((*d, decompressed));
		}
		let crmb = CrateRecMetaWithBlobs {
			meta,
			blobs
		};
		let ccb = CrateContentBlobs::from_meta_with_blobs(crmb);
		Some(ccb.to_archive_file())
	}
}

/// Tasks that can be executed in parallel
enum ParallelTask {
	ObtainCrateContentBlobs(String, Vec<u8>, Digest),
	CompressBlob(Digest, Vec<u8>),
	CreateMultiBlob(Vec<(Digest, Vec<u8>)>),
}

/// Tasks that need blocking access to the blob storage
enum BlockingTask {
	StoreCrateUndeduplicated(String, Vec<u8>),
	StoreCrateContentBlobs(String, CrateContentBlobs),
	StoreBlob(Digest, Vec<u8>),
	StoreMultiBlob(Digest, Vec<Digest>, Vec<u8>),
}

fn handle_parallel_task<ET :FnMut(BlockingTask)>(task :ParallelTask, mut emit_task :ET) {
	match task {
		ParallelTask::ObtainCrateContentBlobs(crate_file_name, crate_archive_file, digest) => {
			match CrateContentBlobs::from_archive_file(&crate_archive_file[..]) {
				Ok(ccb) => {
					if digest == ccb.digest_of_reconstructed() {
						emit_task(BlockingTask::StoreCrateContentBlobs(crate_file_name, ccb));
					} else {
						// Digest mismatch
						emit_task(BlockingTask::StoreCrateUndeduplicated(crate_file_name, crate_archive_file));
					}
				},
				Err(_) => {
					// Error during CrateContentBlobs creation... most likely invalid gz file or sth
					emit_task(BlockingTask::StoreCrateUndeduplicated(crate_file_name, crate_archive_file));
				},
			};
		},
		ParallelTask::CompressBlob(d, blob) => {
			let mut gz_enc = GzBuilder::new().read(blob.as_slice(), Compression::best());
			let mut buffer_compressed = Vec::new();
			io::copy(&mut gz_enc, &mut buffer_compressed).unwrap();

			emit_task(BlockingTask::StoreBlob(d, buffer_compressed));
		},
		ParallelTask::CreateMultiBlob(blobs) => {
			let mut root_blob = None;
			let mut diff_list = Vec::new();
			let mut last :Option<(Digest, &str)> = None;
			for (digest, blob) in blobs.iter() {
				let s = ::std::str::from_utf8(blob).unwrap();
				// TODO don't unwrap
				if let Some(l) = last.take() {
					let diff = Diff::from_texts_nl(&l.1, &s);
					diff_list.push((l.0, *digest, diff));
				} else {
					root_blob = Some((*digest, s.to_string()));
				}
				last = Some((*digest, s));
			}

			let mb = MultiBlob {
				root_blob : root_blob.unwrap(),
				diff_list,
			};
			let mut mb_blob = Vec::new();
			mb.serialize(&mut mb_blob).unwrap();

			let mut hctx = HashCtx::new();
			io::copy(&mut mb_blob.as_slice(), &mut hctx).unwrap();
			let multi_blob_digest = hctx.finish_and_get_digest();

			let mut gz_enc = GzBuilder::new().read(mb_blob.as_slice(), Compression::best());
			let mut buffer_compressed = Vec::new();
			io::copy(&mut gz_enc, &mut buffer_compressed).unwrap();

			let digests = blobs.iter()
				.map(|(digest, _b)| *digest)
				.collect::<Vec<_>>();

			let task = BlockingTask::StoreMultiBlob(multi_blob_digest, digests, buffer_compressed);
			emit_task(task);
		},
	}
}

fn handle_blocking_task<ET :FnMut(ParallelTask), S :Read + Seek + Write>(task :BlockingTask,
		blob_store :&mut BlobStorage<S>, blobs_to_store :&mut HashSet<Digest>,
		mut emit_task :ET) {
	match task {
		BlockingTask::StoreCrateUndeduplicated(crate_file_name, crate_blob) => {
			// TODO
		},
		BlockingTask::StoreCrateContentBlobs(crate_file_name, ccb) => {
			let CrateRecMetaWithBlobs { meta, blobs } = ccb.into_meta_with_blobs();
			for entry in blobs {
				let entry_digest = entry.0;
				if blobs_to_store.insert(entry_digest) {
					emit_task(ParallelTask::CompressBlob(entry_digest, entry.1));
				}
			}
			// emit a blob for meta as well
			let mut meta_blob = Vec::new();
			meta.serialize(&mut meta_blob).unwrap();
			let mut meta_blob_hctx = HashCtx::new();
			io::copy(&mut meta_blob.as_slice(), &mut meta_blob_hctx).unwrap();
			let meta_blob_digest = meta_blob_hctx.finish_and_get_digest();
			// The blob digest may be already present, e.g. if
			// we had been writing this particular crate into the
			// BlobStorage previously. In order to be on the safe
			// side, check for existence before inserting into
			// the blob storage.
			if blobs_to_store.insert(meta_blob_digest) {
				emit_task(ParallelTask::CompressBlob(meta_blob_digest, meta_blob));
			}
			// enter the meta blob into the blob storage
			blob_store.name_index.insert(crate_file_name, meta_blob_digest);

		},
		BlockingTask::StoreBlob(d, blob) => {
			let actually_added = blob_store.insert(d, &blob).unwrap();
			// If the blob is already present, it indicates a bug because
			// we are supposed to check for presence before we ask for the
			// blob to be compressed. If we would just shrug this off, we'd
			// waste cycles spent on compressing the blobs.
			assert!(actually_added, "Tried to insert a blob into the storage that was already present");
		},
		BlockingTask::StoreMultiBlob(mblob_digest, digests, buf_compressed) => {
			let actually_added = blob_store.insert(mblob_digest, &buf_compressed).unwrap();
			for d in digests.iter() {
				blob_store.digest_to_multi_blob.insert(*d, mblob_digest);
			}
			// If the blob is already present, it indicates a bug because
			// we are supposed to check for presence before we ask for the
			// blob to be compressed. If we would just shrug this off, we'd
			// waste cycles spent on compressing the blobs.
			assert!(actually_added, "Tried to insert a blob into the storage that was already present");
		},
	}
}
