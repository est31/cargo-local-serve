
use std::io::{Read, Write, Result as IoResult};
use std::collections::HashMap;
use byteorder::{ReadBytesExt, WriteBytesExt, BigEndian};
use super::hash_ctx::Digest;

pub struct BlobStorage {
	/// Index that maps (crate) names to digests
	///
	/// Note that not all blobs are present in this index, only those that represent
	/// a crate.
	pub index :HashMap<String, Digest>,
	pub blobs :HashMap<Digest, Vec<u8>>,
}

fn write_delim_byte_slice<W :Write>(mut wtr :W, sl :&[u8]) -> IoResult<()> {
	try!(wtr.write_u64::<BigEndian>(sl.len() as u64));
	try!(wtr.write(sl));
	Ok(())
}
fn read_delim_byte_slice<R :Read>(mut rdr :R) -> IoResult<Vec<u8>> {
	let len = try!(rdr.read_u64::<BigEndian>());
	let mut res = vec![0; len as usize];
	try!(rdr.read_exact(&mut res));
	Ok(res)
}

const BLOB_MAGIC :u64 = 0x42_4C_4F_42_53_54_52_45;

impl BlobStorage {
	pub fn new() -> Self {
		BlobStorage {
			index : HashMap::new(),
			blobs : HashMap::new(),
		}
	}
	pub fn insert(&mut self, name :Option<String>, digest :Digest, content :Vec<u8>) {
		if let Some(n) = name {
			self.index.insert(n, digest);
		}
		self.blobs.insert(digest, content);
	}
	pub fn write_to_file<W :Write>(&self, mut wtr :W) -> IoResult<()> {
		try!(wtr.write_u64::<BigEndian>(BLOB_MAGIC));
		try!(wtr.write_u64::<BigEndian>(self.index.len() as u64));
		for (s,d) in self.index.iter() {
			try!(write_delim_byte_slice(&mut wtr, s.as_bytes()));
			try!(wtr.write(d));
		}
		try!(wtr.write_u64::<BigEndian>(self.blobs.len() as u64));
		for (d, b) in self.blobs.iter() {
			try!(wtr.write(d));
			try!(write_delim_byte_slice(&mut wtr, b));
		}
		Ok(())
	}
	pub fn read_from_file<R :Read>(mut rdr :R) -> IoResult<Self> {
		let magic = try!(rdr.read_u64::<BigEndian>());
		// TODO return Err instead of panicing!!
		assert_eq!(magic, BLOB_MAGIC);
		let index_len = try!(rdr.read_u64::<BigEndian>());
		let mut index = HashMap::new();
		for _ in 0 .. index_len {
			let s_bytes = try!(read_delim_byte_slice(&mut rdr));
			let mut d :Digest = [0; 32];
			try!(rdr.read_exact(&mut d));
			let s = String::from_utf8(s_bytes).unwrap();
			// TODO return Err instead of panicing
			index.insert(s, d);
		}

		let blobs_len = try!(rdr.read_u64::<BigEndian>());
		let mut blobs = HashMap::new();
		for _ in 0 .. blobs_len {
			let mut d :Digest = [0; 32];
			try!(rdr.read_exact(&mut d));
			let blob_content = try!(read_delim_byte_slice(&mut rdr));
			blobs.insert(d, blob_content);
		}
		Ok(BlobStorage {
			index,
			blobs,
		})
	}
}
