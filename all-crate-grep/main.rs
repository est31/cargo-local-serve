extern crate all_crate_storage;
extern crate grep;

use std::env;
use std::path::Path;
use all_crate_storage::registry::registry;
use all_crate_storage::crate_storage::{CrateSource, FileTreeStorage};
use self::registry::{Registry, AllCratesJson};

use std::thread;
use std::sync::mpsc::{sync_channel, SyncSender};

use grep::regex::RegexMatcher;
use grep::matcher::Matcher;

fn run(tx :SyncSender<(usize, usize, String)>, acj :&AllCratesJson,
		total_file_count :usize, t :usize, tc :usize,
		grepper :&RegexMatcher, storage_base :&Path) {
	let mut ctr = 0;

	macro_rules! pln {
		($($v:expr),*) => {
			tx.send((ctr, total_file_count, format!($($v),*))).unwrap();
		}
	}

	let mut crate_source = FileTreeStorage::new(storage_base);

	for &(ref name, ref versions) in acj.iter() {

		for ref v in versions.iter() {
			ctr += 1;
			/*if ctr != 21899 {
				continue;
			}*/
			if ctr % tc != t {
				continue;
			}

			let mut fh = match crate_source.get_crate_handle_nv(name.to_owned(), v.version.clone()) {
				Some(f) => f,
				None => {
					pln!("Version {} of crate {} not mirrored", v.version, name);
					continue
				},
			};

			let mut match_found = false;
			fh.crate_file_handle.map_all_files(|file_path, file| {
				if let (Some(file_path), Some(file)) = (file_path, file) {
					if let Ok(Some(_)) = grepper.find(&file) {
						pln!("Match found in {} v {} file {}", name, v.version, file_path);
						match_found = true;
					}
				}
			});
			if !match_found {
				pln!("No match in {} v {}", name, v.version);
			}
		}
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

	let needle = env::args().nth(1).expect("expected search term");
	println!("Search term '{}'", needle);
	let grepper = RegexMatcher::new_line_matcher(&needle).unwrap();

	let (tx, rx) = sync_channel(10);

	let thread_count = 8;
	for v in 0..thread_count {
		let tx = tx.clone();
		let acj = acj.clone();
		let grepper = grepper.clone();
		let storage_base = storage_base.clone();
		thread::spawn(move || {
			run(tx, &acj, total_file_count, v, thread_count,
				&grepper, &storage_base);
		});
	}
	std::mem::drop(tx);
	while let Ok((ctr, tc, s)) = rx.recv() {
		println!("[{}/{}] {}", ctr, tc, s);
	}
}
