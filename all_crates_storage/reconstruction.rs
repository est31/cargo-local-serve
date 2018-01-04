
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
use byteorder::{ReadBytesExt, WriteBytesExt, BigEndian};
use std::mem;
use std::u64;
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
		io::copy(&mut reconstructed.as_slice(), &mut hash_ctx).unwrap();
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

	pub(crate) fn from_meta_with_blobs(m :CrateRecMetaWithBlobs) -> Self {
		let entries = m.meta.entry_metadata.into_iter()
			.zip(m.blobs.into_iter())
			.map(|((h, _), (_, b))| (h, b))
			.collect::<Vec<_>>();
		CrateContentBlobs {
			gz_file_name : m.meta.gz_file_name,
			gz_os : m.meta.gz_os,
			entries,
		}
	}
}

impl CrateRecMetadata {
	pub fn deserialize<R :io::Read>(mut rdr :R) -> io::Result<CrateRecMetadata> {
		let gz_file_name_len = try!(rdr.read_u64::<BigEndian>());
		let gz_file_name = if gz_file_name_len == u64::MAX {
			None
		} else {
			let mut gfn = vec![0; gz_file_name_len as usize];
			try!(rdr.read_exact(&mut gfn));
			Some(gfn)
		};
		let gz_os = try!(rdr.read_u8());
		let entry_count = try!(rdr.read_u64::<BigEndian>()) as usize;
		let mut entry_metadata = Vec::with_capacity(entry_count);
		for _ in 0 .. entry_count {
			let mut hdr = Box::new([0; 512]);
			{
				let hdr_ref :&mut [u8; 512] = &mut hdr;
				try!(rdr.read_exact(hdr_ref));
			}
			let mut digest = [0; 32];
			try!(rdr.read_exact(&mut digest));
			entry_metadata.push((hdr, digest));
		}
		Ok(CrateRecMetadata {
			gz_file_name,
			gz_os,
			entry_metadata,
		})
	}
	pub fn serialize<W :io::Write>(&self, mut wtr :W) -> io::Result<()> {
		if let Some(ref name) = self.gz_file_name {
			try!(wtr.write_u64::<BigEndian>(name.len() as u64));
			try!(wtr.write(&name));
		} else {
			try!(wtr.write_u64::<BigEndian>(u64::MAX));
		}
		try!(wtr.write_u8(self.gz_os));
		try!(wtr.write_u64::<BigEndian>(self.entry_metadata.len() as u64));
		for entry in self.entry_metadata.iter() {
			let hdr_ref :&[u8; 512] = &entry.0;
			try!(wtr.write(hdr_ref));
			try!(wtr.write(&entry.1));
		}
		Ok(())
	}
}
