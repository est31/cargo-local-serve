extern crate all_crates_storage;
extern crate semver;

use std::fs::{self, OpenOptions};
use std::env;
use all_crates_storage::registry::registry;
use all_crates_storage::blob_crate_storage::BlobCrateStorage;
use all_crates_storage::crate_storage::CrateSource;
use self::registry::{Registry, AllCratesJson};
use all_crates_storage::multi_blob_crate_storage::GraphOfBlobs;

fn main() {
	println!("Loading all crates json...");
	let registry = Registry::from_name("github.com-1ecc6299db9ec823").unwrap();
	let acj :AllCratesJson = registry.get_all_crates_json().unwrap();
	let total_file_count :usize = acj.iter().map(|&(_, ref v)| v.len()).sum();

	println!("The target is {} files.", total_file_count);
	let storage_con_base = env::current_dir().unwrap().join("crate-constr-archives");

	fs::create_dir_all(&storage_con_base).unwrap();

	let f = OpenOptions::new()
		.read(true)
		.open(storage_con_base.join("crate_storage")).unwrap();
	let mut cst = BlobCrateStorage::new(f).unwrap();


	//let graph = GraphOfBlobs::from_crate_source(&acj, &mut cst);
	let graph = GraphOfBlobs::from_blob_crate_storage(&acj, &mut cst);
	println!("root number {}", graph.roots.len());
}
