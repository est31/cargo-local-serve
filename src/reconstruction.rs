
/*!
reconstruction of .crate files

In order to achieve gains from doing per-file deduplication and diffing,
we need to be able to reconstruct the exact sha-256-hash matching .crate
files.
*/

use flate2::{Compression, GzBuilder};
use flate2::read::GzDecoder;
use tar::{Archive, Header, Builder as TarBuilder};
use std::mem;
use std::io;

pub struct CrateContentBlobs {
	gz_file_name :Option<Vec<u8>>,
	gz_os :u8,
	entries :Vec<(Box<[u8; 512]>, Vec<u8>)>,
}

impl CrateContentBlobs {
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
	pub fn to_archive_file(self) -> Vec<u8> {
		let mut res = Vec::new();
		let gz_bld = GzBuilder::new()
			.operating_system(self.gz_os);
		let gz_bld = if let Some(filen) = self.gz_file_name {
			gz_bld.filename(filen)
		} else {
			gz_bld
		};
		{
			let mut gz_enc = gz_bld.write(&mut res, Compression::best());
			let mut bld = TarBuilder::new(&mut gz_enc);
			for entry in self.entries {
				let hdr :&Header = unsafe {
					mem::transmute(entry.0)
				};
				let content_sl :&[u8] = &entry.1;
				bld.append(&hdr, content_sl).unwrap();
			}
		}
		res
	}
}
