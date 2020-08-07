extern crate all_crate_storage;
extern crate flate2;
extern crate tar;
extern crate pbr;

use std::fs::File;
use std::io;
use std::env;
use std::path::Path;
use all_crate_storage::registry::registry;
use all_crate_storage::hash_ctx::HashCtx;
use self::registry::{Registry, AllCratesJson};
use flate2::{Compression, GzBuilder};
use flate2::read::GzDecoder;
use tar::Archive;

use std::thread;
use std::sync::mpsc::{sync_channel, SyncSender};
use std::collections::HashMap;

use pbr::MultiBar;

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

#[derive(Clone)]
struct Message {
	t :usize,
	ctr :usize,
	total_file_count :usize,
	msg :Msg,
}
#[derive(Clone)]
enum Msg {
	Text(String),
	File(String, String, u64, u64),
	Done,
}

fn run(tx :SyncSender<Message>, acj :&AllCratesJson,
		total_file_count :usize, t :usize, tc :usize,
		storage_base :&Path) {
	let mut ctr = 0;

	macro_rules! msg {
		($msg:expr) => {
			tx.send(Message {
				t, ctr, total_file_count,
				msg : $msg,
			}).unwrap();
		}
	}
	macro_rules! pln {
		($($v:expr),*) => {
			msg!(Msg::Text(format!($($v),*)));
		}
	}

	for &(ref name, ref versions) in acj.iter() {
		let name_path = storage_base.join(registry::obtain_crate_name_path(name));
		//fs::create_dir_all(&name_c_path).unwrap();

		if BLACKLIST.contains(&&name[..]) {
			ctr += versions.len();
			continue;
		}

		for v in versions.iter() {
			ctr += 1;
			/*if ctr != 21899 {
				continue;
			}*/
			if ctr % tc != t {
				continue;
			}
			let crate_file_path = name_path
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
								expected: '{}' was: '{}'",
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
			//pln!("{:?}", gz_dec.header());
			let mut archive = Archive::new(gz_dec);
			//gz_dec.

			for entry in archive.entries().expect("no") {
				let mut entry = match entry {
					Ok(e) => e,
					Err(e) => {
						pln!("ERROR FOR {} v{}: {:?}", name, v.version, e);
						break;
					},
				};
				let mut buffer = Vec::<u8>::new();
				match io::copy(&mut entry, &mut buffer) {
					Ok(_) => (),
					Err(e) => pln!("ERROR FOR {} v{}: {:?}", name, v.version, e),
				}
				let mut ring_ctx = HashCtx::new();
				let mut sl :&[u8] = &buffer;
				io::copy(&mut sl, &mut ring_ctx).unwrap();
				let hash_str = ring_ctx.finish_and_get_digest_hex();
				let sl :&[u8] = &buffer;
				let mut gz_enc = GzBuilder::new().read(sl, Compression::best());
				let mut buffer_compressed = Vec::<u8>::new();
				io::copy(&mut gz_enc, &mut buffer_compressed).unwrap();
				let compressed_size = buffer_compressed.len() as u64;
				let path_str = entry.path().unwrap().to_str().unwrap().to_string();
				let size = entry.header().entry_size().unwrap();
				msg!(Msg::File(hash_str, path_str, size, compressed_size));
			}
		}
		/*if ctr > 10_000 {
			pln!("abort");
			break;
		}*/
	}
	msg!(Msg::Done);
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
	std::mem::drop(tx);
	let mut h_map = HashMap::new();
	let mut file_count = 0;
	let mut threads_running = thread_count;

	let mb = MultiBar::new();
	mb.println("Analyzing all crates");
	let mut p_list = (0..thread_count)
		.map(|_| mb.create_bar((total_file_count/thread_count) as _))
		.collect::<Vec<_>>();

	thread::spawn(move || {
		mb.listen();
		println!("all bars done!");
	});

	while let Ok(msg) = rx.recv() {
		match msg.msg {
			Msg::Text(_s) => {
				//println!("[{}/{}] {}", msg.ctr, msg.total_file_count, s),
				p_list[msg.t].inc();
			}
			Msg::File(h, path, size, size_c) => {
				file_count += 1;
				let l = h_map.entry(h).or_insert((Vec::new(), size, size_c));
				l.0.push(path);
			},
			Msg::Done => {
				//println!("Thread {} done", msg.t);
				p_list[msg.t].finish();
				threads_running -= 1;
				if threads_running == 0 {
					break;
				}
			},
		}
	}
	let mut hashes = h_map.into_iter().collect::<Vec<_>>();
	hashes.sort_by_key(|&(_, ref l)| l.0.len() as u64 * l.1);
	hashes.reverse();

	println!("File count: {}", file_count);
	println!("Number of unique file hashes: {}", hashes.len());
	println!("Total size uncompressed: {}", hashes.iter()
		.map(|&(_, ref l)| l.0.len() as u64 * l.1).sum::<u64>());
	println!("Uncompressed size of unique files: {}", hashes.iter()
		.map(|&(_, ref l)| l.1).sum::<u64>());
	println!("Compressed size of unique files: {}", hashes.iter()
		.map(|&(_, ref l)| l.2).sum::<u64>());
	println!("top used hashes: {:?}",
		hashes[0..40].iter().map(|&(_, ref l)| (l.0[0].clone(), l.0.len(),
			l.1, l.0.len() as u64 * l.1)).collect::<Vec<_>>());
}
