
use super::blob_storage::BlobStorage;
use super::hash_ctx::Digest;
use super::reconstruction::{CrateContentBlobs, CrateRecMetadata, CrateRecMetaWithBlobs};
use flate2::{Compression, GzBuilder};
use byteorder::{ReadBytesExt, WriteBytesExt, BigEndian};
use std::io;
use std::u64;

pub struct CrateStorage {
	b :BlobStorage,
}

impl CrateStorage {
	pub fn new() -> CrateStorage {
		CrateStorage {
			b : BlobStorage::new(),
		}
	}
	pub fn add_crate(&mut self, crate_file_name :String, crate_data :Vec<u8>,
			hash :Digest) {
		// TODO
		// 1. obtain crate file headers and
		// 2. test restorability of .crate file
		// 3. if not restorable, store .crate directly. done!
		// 4. if restorable, obtain the list of unstored blobs (crate file headers are a blob)
		// 5. compress each unstored blob
		// 6. add all unstored blobs to the blob storage
		// Steps 1,2,5 are the computationally expensive ones and
		// can be executed independently, in a separate thread.
		// Steps 3,4,6 require access to the blob storage,
		// but they are not computationally expensive.
		//
		// Later optimisations:
		// * pass that computes the diff between similar blobs, trying whether that's smaller.
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
			// TODO emit a blob for meta as well

		},
		BlockingTask::StoreBlob(d, blob) => {
			// TODO
		},
	}
}
