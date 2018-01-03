extern crate all_crates_storage;

use std::io;
use std::env;
use std::path::Path;
use all_crates_storage::registry::registry;
use all_crates_storage::reconstruction::{CrateContentBlobs};
use all_crates_storage::hash_ctx::{HashCtx, get_digest_hex};
use all_crates_storage::crate_storage::{CrateSource, FileTreeStorage};
use self::registry::{Registry, AllCratesJson};

use std::thread;
use std::sync::mpsc::{sync_channel, SyncSender};

// corrupt deflate stream error blacklist
// TODO: does having it make any sense?
// https://github.com/rust-lang/cargo/issues/1465
const BLACKLIST :&[&str] = &[
	/*
	"cobs", // v0.1.0
	"curl-sys", // v0.1.0
	"expat-sys", // v2.1.0
	"flate2", // v0.2.2
	"libbreakpad-client-sys", // v0.1.0
	"ppapi", // v0.0.1
	"ruplicity", // v0.1.0
	"rustc-serialize", // v0.3.8
	*/
];

/*

[21898/71082] DIFF FAIL for forust v0.1.0!
Diffoscope output:
-00000780: 7080 03fc 42f8 0fbd 883a 3300 2400 00    p...B....:3.$..
+00000780: 7080 03fc 42f8 0fbd 883a 3300 2400 000a  p...B....:3.$...

[29482/71082] DIFF FAIL for helianto v0.1.0-beta1!
│ -gzip compressed data, was "helianto-0.1.0-beta1.crate", max compression, from Unix
│ +gzip compressed data, was "helianto-0.1.0-beta1.crate", last modified: Wed Jan 13 23:45:28 2016, max compression, from Unix
*/
fn run(tx :SyncSender<(usize, usize, String)>, acj :&AllCratesJson,
		total_file_count :usize, t :usize, tc :usize,
		storage_base :&Path) {
	let mut ctr = 0;

	macro_rules! pln {
		($($v:expr),*) => {
			tx.send((ctr, total_file_count, format!($($v),*))).unwrap();
		}
	}

	let crate_source = FileTreeStorage::new(storage_base);

	for &(ref name, ref versions) in acj.iter() {

		if BLACKLIST.contains(&&name[..]) {
			ctr += versions.len();
			continue;
		}

		for ref v in versions.iter() {
			ctr += 1;
			/*if ctr != 21899 {
				continue;
			}*/
			if ctr % tc != t {
				continue;
			}
			let crate_blob = crate_source.get_crate_nv(name.clone(), v.version.clone());
			let crate_blob = if let Some(crate_blob) = crate_blob {
				// verify the checksum
				let mut ring_ctx = HashCtx::new();
				io::copy(&mut crate_blob.as_slice(), &mut ring_ctx).unwrap();
				let hash_str = ring_ctx.finish_and_get_digest_hex();
				if hash_str == v.checksum {
					// everything is fine!
					crate_blob
				} else {
					pln!("Checksum mismatch for {} v{}. \
							Ignoring. expected: '{}' was: '{}'",
							name, v.version, v.checksum, hash_str);
					// Ignore
					continue;
				}
			} else {
				pln!("Crate not present for {} v{}", name, v.version);
				// Ignore
				continue;
			};
			pln!("Diffing {} v{}", name, v.version);

			// Do the diffing.

			// Create the blobs
			let archive_blobs = match CrateContentBlobs::from_archive_file(crate_blob.as_slice()) {
				Ok(b) => b,
				Err(e) => {
					pln!("ERROR FOR {} v{}: {:?}", name, v.version, e);
					continue;
				},
			};

			// Reconstruct the file and compute the digest
			let digest_reconstructed = archive_blobs.digest_of_reconstructed();

			// Compare the digest
			let hash_str = get_digest_hex(digest_reconstructed);
			if hash_str != v.checksum {
				pln!("DIFF FAIL for {} v{}!", name, v.version);
			}
		}
		/*if ctr > 100 {
			pln!("abort");
			break;
		}*/
	}
	println!("thread {} done", t);
}

fn main() {
	println!("Loading all crates json...");
	let registry = Registry::from_name("github.com-1ecc6299db9ec823").unwrap();
	let acj :AllCratesJson = registry.get_all_crates_json().unwrap();
	let total_file_count :usize = acj.iter().map(|&(_, ref v)| v.len()).sum();

	println!("The target is {} files.", total_file_count);
	let storage_base = env::current_dir().unwrap().join("crate-archives");
	println!("Using directory {} to load the files from.",
		storage_base.to_str().unwrap());

	let (tx, rx) = sync_channel(10);

	let thread_count = 8;
	for v in 0..thread_count {
		let tx = tx.clone();
		let acj = acj.clone();
		let storage_base = storage_base.clone();
		thread::spawn(move || {
			run(tx, &acj, total_file_count, v, thread_count,
				&storage_base);
		});
	}
	while let Ok((ctr, tc, s)) = rx.recv() {
		println!("[{}/{}] {}", ctr, tc, s);
	}
}
