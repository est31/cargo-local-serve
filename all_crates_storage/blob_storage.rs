
use std::io::{Read, Write, Seek, SeekFrom, Result as IoResult, ErrorKind};
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use byteorder::{ReadBytesExt, WriteBytesExt, BigEndian};
use super::hash_ctx::Digest;

pub struct BlobStorage<S> {
	/// Index that maps (crate) names to digests
	///
	/// Note that not all blobs are present in this index, only those that represent
	/// a crate.
	pub name_index :HashMap<String, Digest>,
	blob_offsets :HashMap<Digest, u64>,
	storage :S,
	index_offset :u64,
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

impl<S :Read + Seek> BlobStorage<S> {
	pub fn empty(storage :S) -> Self {
		BlobStorage {
			name_index : HashMap::new(),
			blob_offsets : HashMap::new(),

			storage,
			// TODO don't hardcode this number somehow (get the length of the header)
			index_offset : 64,
		}
	}
	pub fn new(mut storage :S) -> IoResult<Self> {
		try!(storage.seek(SeekFrom::Start(0)));
		match storage.read_u64::<BigEndian>() {
			Ok(v) if v == BLOB_MAGIC => BlobStorage::load(storage),
			Ok(_) => panic!("Invalid header"),
			Err(ref e) if e.kind() == ErrorKind::UnexpectedEof => Ok(BlobStorage::empty(storage)),
			Err(e) => Err(e),
		}
	}
	pub fn load(mut storage :S) -> IoResult<Self> {
		try!(storage.seek(SeekFrom::Start(0)));
		let index_offset = try!(read_hdr(&mut storage));
		try!(storage.seek(SeekFrom::Start(index_offset)));
		let name_index = try!(read_name_idx(&mut storage));
		let blob_offsets = try!(read_offset_table(&mut storage));
		Ok(BlobStorage {
			name_index,
			blob_offsets,

			storage,
			index_offset,
		})
	}

	pub fn has(&self, digest :&Digest) -> bool {
		self.blob_offsets.get(digest).is_some()
	}
	pub fn get(&mut self, digest :&Digest) -> IoResult<Option<Vec<u8>>> {
		let blob_offs = match self.blob_offsets.get(digest) {
			Some(d) => *d,
			None => return Ok(None),
		};
		try!(self.storage.seek(SeekFrom::Start(blob_offs)));
		let content = try!(read_delim_byte_slice(&mut self.storage));
		Ok(Some(content))
	}
}

impl<S :Seek + Write> BlobStorage<S> {
	pub fn insert_named_blob(&mut self, name :Option<String>, digest :Digest, content :&[u8]) -> IoResult<()> {
		if let Some(n) = name {
			self.name_index.insert(n, digest);
		}
		try!(self.insert(digest, &content));
		Ok(())
	}
	pub fn insert(&mut self, digest :Digest, content :&[u8]) -> IoResult<bool> {
		let e = self.blob_offsets.entry(digest);
		match e {
			Entry::Occupied(o) => return Ok(false),
			Entry::Vacant(v) => v.insert(self.index_offset),
		};
		try!(self.storage.seek(SeekFrom::Start(self.index_offset)));
		try!(write_delim_byte_slice(&mut self.storage, content));
		self.index_offset = try!(self.storage.seek(SeekFrom::Current(0)));
		Ok(true)
	}
	pub fn write_header_and_index(&mut self) -> IoResult<()> {
		try!(self.storage.seek(SeekFrom::Start(0)));
		try!(write_hdr(&mut self.storage, self.index_offset));
		try!(self.storage.seek(SeekFrom::Start(self.index_offset)));
		try!(write_name_idx(&mut self.storage, &self.name_index));
		try!(write_offset_table(&mut self.storage, &self.blob_offsets));
		Ok(())
	}
}

const BLOB_MAGIC :u64 = 0x42_4C_4F_42_53_54_52_45;

fn read_hdr<R :Read>(mut rdr :R) -> IoResult<u64> {
	let magic = try!(rdr.read_u64::<BigEndian>());
	// TODO return Err instead of panicing
	assert_eq!(magic, BLOB_MAGIC);
	let index_offset = try!(rdr.read_u64::<BigEndian>());
	Ok(index_offset)
}
fn write_hdr<W :Write>(mut wtr :W, index_offset :u64) -> IoResult<()> {
	try!(wtr.write_u64::<BigEndian>(BLOB_MAGIC));
	try!(wtr.write_u64::<BigEndian>(index_offset));
	Ok(())
}
fn read_offset_table<R :Read>(mut rdr :R) -> IoResult<HashMap<Digest, u64>> {
	let len = try!(rdr.read_u64::<BigEndian>());
	let mut tbl = HashMap::new();
	for _ in 0 .. len {
		let mut d :Digest = [0; 32];
		try!(rdr.read_exact(&mut d));
		let offset = try!(rdr.read_u64::<BigEndian>());
		tbl.insert(d, offset);
	}
	Ok(tbl)
}
fn write_offset_table<W :Write>(mut wtr :W, tbl :&HashMap<Digest, u64>) -> IoResult<()> {
	try!(wtr.write_u64::<BigEndian>(tbl.len() as u64));
	for (d, o) in tbl.iter() {
		try!(wtr.write(d));
		try!(wtr.write_u64::<BigEndian>(*o));
	}
	Ok(())
}
fn read_name_idx<R :Read>(mut rdr :R) -> IoResult<HashMap<String, Digest>> {
	let nidx_len = try!(rdr.read_u64::<BigEndian>());
	let mut nidx = HashMap::new();
	for _ in 0 .. nidx_len {
		let s_bytes = try!(read_delim_byte_slice(&mut rdr));
		let mut d :Digest = [0; 32];
		try!(rdr.read_exact(&mut d));
		let s = String::from_utf8(s_bytes).unwrap();
		// TODO return Err instead of panicing
		nidx.insert(s, d);
	}
	Ok(nidx)
}
fn write_name_idx<W :Write>(mut wtr :W, nidx :&HashMap<String, Digest>) -> IoResult<()> {
	try!(wtr.write_u64::<BigEndian>(nidx.len() as u64));
	for (s,d) in nidx.iter() {
		try!(write_delim_byte_slice(&mut wtr, s.as_bytes()));
		try!(wtr.write(d));
	}
	Ok(())
}
