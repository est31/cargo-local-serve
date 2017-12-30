
use super::blob_storage::BlobStorage;
use super::hash_ctx::Digest;

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
	ObtainCrateContentBlobs(String, Vec<u8>),
	CompressBlob(Vec<u8>),
}

/// Tasks that need blocking access to the blob storage
enum BlockingTask {
	StoreCrate(Vec<u8>),
	StoreCrateContentBlobs
	StoreBlob
}

pub struct CrateFileHeaders {
}
