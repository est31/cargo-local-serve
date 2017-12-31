use base64;
use ring::digest::{Context, SHA256};
use std::io;

pub type Digest = [u8; 32];

pub fn get_digest_hex(digest :Digest) -> String {
	base64::encode(&digest)
}

pub fn digest_from_hex(digest :&str) -> Option<Digest> {
	let mut res = [0; 32];

	match base64::decode_config_slice(digest, base64::STANDARD, &mut res) {
		Ok(n) if n == 32 => Some(res),
		_ => None,
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
