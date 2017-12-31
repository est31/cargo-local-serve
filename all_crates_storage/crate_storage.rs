
use super::blob_storage::BlobStorage;
use super::hash_ctx::{HashCtx, Digest};
use super::reconstruction::{CrateContentBlobs, CrateRecMetaWithBlobs};
use super::registry::registry::{CrateIndexJson, AllCratesJson, obtain_crate_name_path};

use flate2::{Compression, GzBuilder};
use std::fs::File;
use std::path::Path;
use std::io;

pub struct CrateStorage {
	b :BlobStorage,
}

impl CrateStorage {
	pub fn new() -> CrateStorage {
		CrateStorage {
			b : BlobStorage::new(),
		}
	}
	pub fn fill_crate_storage_from_disk(&mut self,
			thread_count :u16, acj :&AllCratesJson, storage_base :&Path, progress_callback :fn(&str, &CrateIndexJson)) {
		let crate_iter = acj.iter()
			.flat_map(|&(ref name, ref versions)| {
				let name_path = storage_base.join(obtain_crate_name_path(name));
				let name = name.clone();
				versions.iter().filter_map(move |v| {
					progress_callback(&name, &v);
					let name_str = format!("{}-{}.crate", name, v.version);
					let crate_file_path = name_path.join(&name_str);
					let mut f = match File::open(&crate_file_path) {
						Ok(f) => f,
						Err(_) => {
							return None;
						},
					};
					let mut file_buf = Vec::new();
					io::copy(&mut f, &mut file_buf).unwrap();
					let mut hctx = HashCtx::new();
					io::copy(&mut file_buf.as_slice(), &mut hctx).unwrap();
					let d = hctx.finish_and_get_digest();
					Some((name_str, file_buf, d))
				})
			});
		self.fill_crate_storage_iter(thread_count, crate_iter);
	}
	pub fn fill_crate_storage_iter<I :Iterator<Item = (String, Vec<u8>, Digest)>>(
			&mut self, thread_count :u16, mut crate_iter :I) {
		use std::sync::mpsc::{sync_channel, TrySendError};
		use multiqueue::mpmc_queue;
		use std::time::Duration;
		use std::thread;

		let (bt_tx, bt_rx) = sync_channel(10);
		let (pt_tx, pt_rx) = mpmc_queue(10);
		for tid in 0 .. thread_count {
			let bt_tx = bt_tx.clone();
			let pt_rx = pt_rx.clone();
			thread::spawn(move || {
				while let Ok(task) = pt_rx.recv() {
					handle_parallel_task(task, |bt| bt_tx.send((tid, bt)).unwrap());
				}
			});
		}
		drop(bt_tx);
		pt_rx.unsubscribe();
		let mut par_task_backlog = Vec::new();
		loop {
			let mut done_something = false;
			if par_task_backlog.is_empty() {
				if let Some((n, b, d)) = crate_iter.next() {
					par_task_backlog.push(ParallelTask::ObtainCrateContentBlobs(n, b, d));
					done_something = true;
				}
			}
			if let Ok((tid, task)) = bt_rx.recv_timeout(Duration::new(0, 50_000)) {
				handle_blocking_task(task, &mut self.b,
					|tsk| par_task_backlog.push(tsk));
				done_something = true;
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
	pub fn store<W :Write>(&mut self, wtr :W) -> io::Result<()> {
		self.b.write_to_file(wtr)
	}
}


/// Tasks that can be executed in parallel
enum ParallelTask {
	ObtainCrateContentBlobs(String, Vec<u8>, Digest),
	CompressBlob(Digest, Vec<u8>),
}

/// Tasks that need blocking access to the blob storage
enum BlockingTask {
	StoreCrateUndeduplicated(String, Vec<u8>),
	StoreCrateContentBlobs(String, CrateContentBlobs),
	StoreBlob(Digest, Vec<u8>),
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
			let mut blob_rdr :&[u8] = &blob;
			let mut gz_enc = GzBuilder::new().read(blob_rdr, Compression::best());
			let mut buffer_compressed = Vec::new();
			io::copy(&mut gz_enc, &mut buffer_compressed).unwrap();

			emit_task(BlockingTask::StoreBlob(d, buffer_compressed));
		},
	}
}

fn handle_blocking_task<ET :FnMut(ParallelTask)>(task :BlockingTask, blob_store :&mut BlobStorage, mut emit_task :ET) {
	match task {
		BlockingTask::StoreCrateUndeduplicated(crate_file_name, crate_blob) => {
			// TODO
		},
		BlockingTask::StoreCrateContentBlobs(crate_file_name, ccb) => {
			let CrateRecMetaWithBlobs { meta, blobs } = ccb.into_meta_with_blobs();
			for entry in blobs {
				let entry_digest = entry.0;
				if blob_store.blobs.get(&entry_digest).is_none() {
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
			if blob_store.blobs.get(&meta_blob_digest).is_none() {
				emit_task(ParallelTask::CompressBlob(meta_blob_digest, meta_blob));
			}
			// enter the meta blob into the blob storage
			blob_store.index.insert(crate_file_name, meta_blob_digest);

		},
		BlockingTask::StoreBlob(d, blob) => {
			blob_store.blobs.insert(d, blob);
		},
	}
}
