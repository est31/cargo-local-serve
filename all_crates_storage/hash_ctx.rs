use hex;
use ring::digest::{Context, SHA256};
use std::io;

pub type Digest = [u8; 32];

pub fn get_digest_hex(digest :Digest) -> String {
	hex::encode(&digest)
}

pub fn digest_from_hex(digest :&str) -> Option<Digest> {
	match hex::decode(&digest) {
		Ok(v) => {
			let mut res = [0; 32];
			res.copy_from_slice(&v[..32]);
			Some(res)
		},
		Err(_) => None,
	}
}

/// SHA-256 hash context that impls Write
pub struct HashCtx(Context);

impl io::Write for HashCtx {
	fn write(&mut self, data: &[u8]) -> Result<usize, io::Error> {
		self.0.update(data);
		Ok(data.len())
	}
	fn flush(&mut self) -> Result<(), io::Error> {
		Ok(())
	}
}

impl HashCtx {
	pub fn new() -> HashCtx {
		HashCtx(Context::new(&SHA256))
	}
	pub fn finish_and_get_digest_hex(self) -> String {
		let digest = self.finish_and_get_digest();
		get_digest_hex(digest)
	}
	pub fn finish_and_get_digest(self) -> Digest {
		let digest = self.0.finish();
		let mut res = [0; 32];
		for i in 0 .. 32 {
			res[i] = digest.as_ref()[i];
		}
		res
	}
}
