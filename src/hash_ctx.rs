use ring::digest::{Context, SHA256};
use std::io;

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
	pub fn init_sha256() -> HashCtx {
		HashCtx(Context::new(&SHA256))
	}
	pub fn finish_and_get_digest_hex(self) -> String {
		let digest = self.0.finish();
		let mut hash_str = String::with_capacity(60);
		for d in digest.as_ref().iter() {
			hash_str += &format!("{:02x}", d);
		}
		hash_str
	}
}
