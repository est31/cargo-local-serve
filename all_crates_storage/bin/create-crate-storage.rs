extern crate all_crates_storage;

use std::io;
use std::env;
use std::path::Path;
use all_crates_storage::registry::registry;
use all_crates_storage::crate_storage::{self, CrateStorage};
use self::registry::{Registry, AllCratesJson};

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

	let thread_count = 8;

	let mut cst = CrateStorage::new();
	cst.fill_crate_storage_from_disk(thread_count, &acj, &storage_base,
		|n, v| println!("Storing {} v {}", n, v.version));
}
