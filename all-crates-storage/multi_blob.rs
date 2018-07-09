/*!
Multiblob storage

This structure stores multiple blobs by storing the
first blob directly, and then expressing the other
blobs via a tree structure, only storing the edges
between the vertices via diffs.
*/

use hash_ctx::Digest;
use super::diff::Diff;
use std::io::Result as IoResult;
use std::io::{Read, Write};
use byteorder::{ReadBytesExt, WriteBytesExt, BigEndian};
use super::blob_storage::{write_delim_byte_slice, read_delim_byte_slice};

pub struct MultiBlob {
	pub(crate) root_blob :(Digest, String),
	/// The tree expressed in DFS traversal form.
	///
	/// In order to get the path to the root,
	/// you should traverse this list in a reverse
	/// fashion.
	pub(crate) diff_list :Vec<(Digest, Digest, Diff)>,
}

impl MultiBlob {
	pub fn get_blob(&self, d :Digest) -> Option<String> {
		// Assemble the diffs
		let mut needed = d;
		let mut diffs = Vec::new();
		for cr in self.diff_list.iter().rev() {
			if cr.0 != needed {
				continue;
			}
			needed = cr.1;
			diffs.push(&cr.2);
		}
		// Resolve the diffs
		if needed != self.root_blob.0 {
			return None;
		}
		let mut res = self.root_blob.1.clone();
		for diff in diffs.iter() {
			res = diff.reconstruct_new(&res);
		}
		Some(res)
	}
	pub fn deserialize<R :Read>(mut rdr :R) -> IoResult<Self> {
		let mut root_digest :Digest = [0; 32];
		try!(rdr.read_exact(&mut root_digest));
		let root_sl = try!(read_delim_byte_slice(&mut rdr));
		// TODO get rid of unwrap here
		let root_s = String::from_utf8(root_sl).unwrap();
		let root_blob = (root_digest, root_s);
		let len = try!(rdr.read_u64::<BigEndian>());
		let mut diff_list = Vec::with_capacity(len as usize);
		for _ in 0 .. len {
			let mut digest_a :Digest = [0; 32];
			try!(rdr.read_exact(&mut digest_a));
			let mut digest_b :Digest = [0; 32];
			try!(rdr.read_exact(&mut digest_b));
			let diff = try!(Diff::deserialize(&mut rdr));
			diff_list.push((digest_a, digest_b, diff));
		}
		Ok(MultiBlob { root_blob, diff_list })
	}
	pub fn serialize<W :Write>(&self, mut wtr :W) -> IoResult<()> {
		try!(wtr.write(&self.root_blob.0));
		try!(write_delim_byte_slice(&mut wtr, self.root_blob.1.as_bytes()));
		try!(wtr.write_u64::<BigEndian>(self.diff_list.len() as u64));
		for d in self.diff_list.iter() {
			try!(wtr.write(&d.0));
			try!(wtr.write(&d.1));
			try!(d.2.serialize(&mut wtr));
		}
		Ok(())
	}
}
