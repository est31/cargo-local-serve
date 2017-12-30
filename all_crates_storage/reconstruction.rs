
/*!
reconstruction of .crate files

In order to achieve gains from doing per-file deduplication and diffing,
we need to be able to reconstruct the exact sha-256-hash matching .crate
files.
*/

use super::hash_ctx::{Digest, HashCtx};
use flate2::{Compression, GzBuilder};
use flate2::read::GzDecoder;
use tar::{Archive, Header, Builder as TarBuilder};
use std::mem;
use std::io;

pub(crate) struct CrateRecMetadata {
	pub(crate) gz_file_name :Option<Vec<u8>>,
	pub(crate) gz_os :u8,
	pub(crate) entry_metadata :Vec<(Box<[u8; 512]>, Digest)>,
}

pub(crate) struct CrateRecMetaWithBlobs {
	pub(crate) meta :CrateRecMetadata,
	pub(crate) blobs :Vec<(Digest, Vec<u8>)>,
}

pub struct CrateContentBlobs {
	gz_file_name :Option<Vec<u8>>,
	gz_os :u8,
	entries :Vec<(Box<[u8; 512]>, Vec<u8>)>,
}

impl CrateContentBlobs {
	/// Creates the CrateContentBlobs structure from a given .crate file
	pub fn from_archive_file<R :io::Read>(archive_rdr :R) -> io::Result<Self> {
		let gz_dec = GzDecoder::new(archive_rdr);
		let gz_file_name = gz_dec.header().unwrap()
			.filename().map(|v| v.to_vec());
		let gz_os = gz_dec.header().unwrap().operating_system();
		let mut archive = Archive::new(gz_dec);
		let mut entries = Vec::new();
		for entry in archive.entries().unwrap().raw(true) {
			let mut entry = try!(entry);
			let hdr_box = Box::new(entry.header().as_bytes().clone());
			let mut content = Vec::new();
			try!(io::copy(&mut entry, &mut content));
			entries.push((hdr_box, content));
		}
		Ok(CrateContentBlobs {
			gz_file_name,
			gz_os,
			entries,
		})
	}

	/// Reconstructs the .crate file from the CrateContentBlobs structure
	pub fn to_archive_file(&self) -> Vec<u8> {
		let mut res = Vec::new();
		let gz_bld = GzBuilder::new()
			.operating_system(self.gz_os);
		let gz_bld = if let Some(filen) = self.gz_file_name.clone() {
			gz_bld.filename(filen)
		} else {
			gz_bld
		};
		{
			let mut gz_enc = gz_bld.write(&mut res, Compression::best());
			let mut bld = TarBuilder::new(&mut gz_enc);
			for entry in &self.entries {
				let hdr_buf = entry.0.clone();
				let hdr :&Header = unsafe {
					mem::transmute(hdr_buf)
				};
				let content_sl :&[u8] = &entry.1;
				bld.append(&hdr, content_sl).unwrap();
			}
		}
		res
	}

	/// Reconstructs the .crate file and obtains its digest
	pub fn digest_of_reconstructed(&self) -> Digest {
		let mut hash_ctx = HashCtx::new();
		let reconstructed = self.to_archive_file();
		let mut reconstructed_rdr :&[u8] = &reconstructed;
		io::copy(&mut reconstructed_rdr, &mut hash_ctx).unwrap();
		hash_ctx.finish_and_get_digest()
	}

	pub(crate) fn into_meta_with_blobs(self) -> CrateRecMetaWithBlobs {
		let mut entry_metadata = Vec::new();
		let mut blobs = Vec::new();
		for entry in self.entries {
			let content = entry.1;
			let mut hash_ctx = HashCtx::new();
			io::copy(&mut content.as_slice(), &mut hash_ctx).unwrap();
			let digest = hash_ctx.finish_and_get_digest();
			entry_metadata.push((entry.0, digest));
			blobs.push((digest, content));
		}
		CrateRecMetaWithBlobs {
			meta : CrateRecMetadata {
				gz_file_name : self.gz_file_name,
				gz_os : self.gz_os,
				entry_metadata,
			},
			blobs,
		}
	}
}
