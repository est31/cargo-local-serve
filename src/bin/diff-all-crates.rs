extern crate cargo_local_serve;
extern crate flate2;
extern crate tar;

use std::fs::{self, File};
use std::io;
use std::env;
use std::path::Path;
use cargo_local_serve::registry::registry;
use cargo_local_serve::hash_ctx::HashCtx;
use self::registry::{Registry, AllCratesJson};
use flate2::{Compression, GzBuilder};
use flate2::read::GzDecoder;
use tar::{Archive, Header, Builder as TarBuilder};

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

struct ArchiveBlob {
	entries :Vec<(Box<[u8; 512]>, Vec<u8>)>,
}

fn gen_archive_blob<R :io::Read>(mut archive :Archive<R>) -> io::Result<ArchiveBlob> {
	let mut entries = Vec::new();
	for entry in archive.entries().unwrap().raw(true) {
		let mut entry = try!(entry);
		let hdr_box = Box::new(entry.header().as_bytes().clone());
		let mut content = Vec::new();
		std::io::copy(&mut entry, &mut content).unwrap();
		entries.push((hdr_box, content));
	}
	Ok(ArchiveBlob {
		entries,
	})
}

impl ArchiveBlob {
	fn to_archive_file(self) -> Vec<u8> {
		let mut res = Vec::new();
		{
			let mut bld = TarBuilder::new(&mut res);
			for entry in self.entries {
				let hdr :&Header = unsafe {
					std::mem::transmute(entry.0)
				};
				let content_sl :&[u8] = &entry.1;
				bld.append(&hdr, content_sl).unwrap();
			}
		}
		res
	}
}

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
		storage_base :&Path, storage_con_base :&Path) {
	let mut ctr = 0;

	macro_rules! pln {
		($($v:expr),*) => {
			tx.send((ctr, total_file_count, format!($($v),*))).unwrap();
		}
	}

	for &(ref name, ref versions) in acj.iter() {
		let name_path = storage_base.join(registry::obtain_crate_name_path(name));
		let name_c_path = storage_con_base.join(registry::obtain_crate_name_path(name));
		//fs::create_dir_all(&name_c_path).unwrap();

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
			let crate_file_path = name_path
				.join(format!("{}-{}.crate", name, v.version));
			let crate_file_c_path = name_c_path
				.join(format!("{}-{}.crate", name, v.version));
			match File::open(&crate_file_path) {
				Ok(mut f) => {
					// verify the checksum
					let mut ring_ctx = HashCtx::new();
					io::copy(&mut f, &mut ring_ctx).unwrap();
					let hash_str = ring_ctx.finish_and_get_digest_hex();
					if hash_str == v.checksum {
						// everything is fine!
					} else {
						pln!("Checksum mismatch for {} v{}. \
								Ignoring. expected: '{}' was: '{}'",
								name, v.version, v.checksum, hash_str);
						// Ignore
						continue;
					}
				},
				Err(_) => {
					pln!("File not found for {} v{}", name, v.version);
					// Ignore
					continue;
				},
			}
			pln!("Diffing {} v{}", name, v.version);

			// Do the diffing.
			let f = File::open(&crate_file_path).unwrap();
			let gz_dec = GzDecoder::new(f);
			let file_name = gz_dec.header().unwrap()
				.filename().map(|v| v.to_vec());
			let os = gz_dec.header().unwrap().operating_system();
			//pln!("{:?}", gz_dec.header());
			let archive_blob = match gen_archive_blob(Archive::new(gz_dec)) {
				Ok(b) => b,
				Err(e) => {
					pln!("ERROR FOR {} v{}: {:?}", name, v.version, e);
					continue;
				},
			};
			let archive_file = archive_blob.to_archive_file();
			let archive_rdr :&[u8] = &archive_file;
			let gz_bld = GzBuilder::new()
				.operating_system(os);
			let gz_bld = if let Some(filen) = file_name {
				gz_bld.filename(filen)
			} else {
				gz_bld
			};
			let mut gz_enc = gz_bld.read(archive_rdr, Compression::best());

			/*let mut f = File::create(&crate_file_c_path).unwrap();
			io::copy(&mut gz_enc, &mut f).unwrap();*/

			let mut ring_ctx = HashCtx::new();
			match io::copy(&mut gz_enc, &mut ring_ctx) {
				Ok(_) => (),
				Err(e) => pln!("ERROR FOR {} v{}: {:?}", name, v.version, e),
			}
			let hash_str = ring_ctx.finish_and_get_digest_hex();
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
	let storage_con_base = env::current_dir().unwrap().join("crate-constr-archives");
	println!("Using directory {} to load the files from.",
		storage_base.to_str().unwrap());

	let (tx, rx) = sync_channel(10);

	let thread_count = 8;
	for v in 0..thread_count {
		let tx = tx.clone();
		let acj = acj.clone();
		let storage_base = storage_base.clone();
		let storage_con_base = storage_con_base.clone();
		thread::spawn(move || {
			run(tx, &acj, total_file_count, v, thread_count,
				&storage_base, &storage_con_base);
		});
	}
	while let Ok((ctr, tc, s)) = rx.recv() {
		println!("[{}/{}] {}", ctr, tc, s);
	}
}
