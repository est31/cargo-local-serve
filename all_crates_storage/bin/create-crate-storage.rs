extern crate all_crates_storage;

use std::fs::{self, OpenOptions};
use std::env;
use all_crates_storage::registry::registry;
use all_crates_storage::blob_crate_storage::BlobCrateStorage;
use all_crates_storage::crate_storage::{FileTreeStorage, CrateStorage};
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

	fs::create_dir_all(&storage_con_base).unwrap();

	let thread_count = 8;

	let fts = FileTreeStorage::new(&storage_base);
	let f = OpenOptions::new()
		.read(true)
		.write(true)
		.create(true)
		.open(storage_con_base.join("crate_storage")).unwrap();
	let mut cst = BlobCrateStorage::new(f).unwrap();
	cst.fill_crate_storage_from_source(thread_count, &acj, &fts,
		|n, v| println!("Storing {} v {}", n, v.version));

	cst.store().unwrap();
}
