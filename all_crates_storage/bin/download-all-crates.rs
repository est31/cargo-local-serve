extern crate all_crates_storage;
extern crate reqwest;
extern crate ring;

use std::fs::{self, File};
use std::io;
use std::env;
use reqwest::Client;
use all_crates_storage::registry::registry;
use self::registry::{Registry, AllCratesJson};
use ring::digest::{Context, SHA256};

struct HashCtx(Context);

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
	fn finish_and_get_digest_hex(self) -> String {
		let digest = self.0.finish();
		let mut hash_str = String::with_capacity(60);
		for d in digest.as_ref().iter() {
			hash_str += &format!("{:02x}", d);
		}
		hash_str
	}
}

/// This is the URL pattern that we are using to download the .crate
/// files from hosting.
///
/// At the moment, DNS records for the domain state that
/// it lives in amazon's "global" region.
///
/// If the URL shouldn't work (any more) for some reason, the more
/// stable URL pattern is this one:
///
/// ```
/// https://crates.io/api/v1/crates/{cratename}/{version}/download
/// ```
/// This URL occurs in the crates.io
///
///
/// But we don't want to fake download statistics, so we directly
/// take the cloudfront URL.
/// The stable URL is simply just a redirect to the cloudfront URL.
macro_rules! download_url_pattern {
	() => {
	"https://d19xqa3lc3clo8.cloudfront.net/crates/{cratename}/{cratename}-{version}.crate"
	};
}

// TODO
// * add modes:
//   * Download only mode, where we don't delete any corrupt crate files
//   * Verification only mode
// * Add a way to change:
//   * Registry name/path
//   * URL pattern
//   * Directory where we download to
// * Experiment with ways to download stuff faster

fn main() {
	println!("Loading all crates json...");
	let registry = Registry::from_name("github.com-1ecc6299db9ec823").unwrap();
	let acj :AllCratesJson = registry.get_all_crates_json().unwrap();
	let total_file_count :usize = acj.iter().map(|&(_, ref v)| v.len()).sum();

	println!("The target is {} files.", total_file_count);
	let storage_base = env::current_dir().unwrap().join("crate-archives");
	println!("Using directory {} to store the files.",
		storage_base.to_str().unwrap());

	let cl = Client::builder().gzip(false).build().unwrap();

	let mut ctr = 0;

	for &(ref name, ref versions) in acj.iter() {
		let name_path = storage_base.join(registry::obtain_crate_name_path(name));

		fs::create_dir_all(&name_path).unwrap();

		for ref v in versions.iter() {
			ctr += 1;
			let crate_file_path = name_path
				.join(format!("{}-{}.crate", name, v.version));
			match File::open(&crate_file_path) {
				Ok(mut f) => {
					// verify the checksum
					let mut ring_ctx = HashCtx(Context::new(&SHA256));
					io::copy(&mut f, &mut ring_ctx).unwrap();
					let hash_str = ring_ctx.finish_and_get_digest_hex();
					if hash_str == v.checksum {
						println!("[{}/{}] Checksum verified for {} v{}",
							ctr, total_file_count, name, v.version);
						continue;
					} else {
						println!("[{}/{}] Checksum mismatch for {} v{}. \
								Deleting. expected: '{}' was: '{}'",
							ctr, total_file_count, name, v.version,
							v.checksum, hash_str);
						fs::remove_file(&crate_file_path).unwrap();
					}
				},
				Err(e) => {
					// TODO check e and if it is anything else than
					// "file not found", make a sad face :)
				},
			}
			println!("[{}/{}] Downloading {} v{}",
				ctr, total_file_count, name, v.version);
			let url = format!(download_url_pattern!(),
				cratename = name,
				version = v.version);
			let mut resp = cl.get(&url).send().unwrap();
			if resp.status().is_success() {
				let mut f = File::create(crate_file_path).unwrap();
				io::copy(&mut resp, &mut f).unwrap();
			} else {
				println!("Got error from server: {}", resp.status());
			}
		}
	}
}
