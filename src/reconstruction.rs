
/*!
reconstruction of .crate files

In order to achieve gains from doing per-file deduplication and diffing,
we need to be able to reconstruct the exact sha-256-hash matching .crate
files.
*/

use tar::{Archive, Header, Builder as TarBuilder};
use std::mem;
use std::io;

pub struct CrateContentBlobs {
	entries :Vec<(Box<[u8; 512]>, Vec<u8>)>,
}

impl CrateContentBlobs {
	pub fn from_archive_file<R :io::Read>(archive_rdr :R) -> io::Result<Self> {
		let mut archive = Archive::new(archive_rdr);
		let mut entries = Vec::new();
		for entry in archive.entries().unwrap().raw(true) {
			let mut entry = try!(entry);
			let hdr_box = Box::new(entry.header().as_bytes().clone());
			let mut content = Vec::new();
			try!(io::copy(&mut entry, &mut content));
			entries.push((hdr_box, content));
		}
		Ok(CrateContentBlobs {
			entries,
		})
	}
	pub fn to_archive_file(self) -> Vec<u8> {
		let mut res = Vec::new();
		{
			let mut bld = TarBuilder::new(&mut res);
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
