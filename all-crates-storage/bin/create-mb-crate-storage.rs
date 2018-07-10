extern crate all_crates_storage;
extern crate semver;

use std::fs::{self, OpenOptions};
use std::env;
use all_crates_storage::registry::registry;
use all_crates_storage::blob_crate_storage::BlobCrateStorage;
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

	let src_f = OpenOptions::new()
		.read(true)
		.open(storage_con_base.join("crate_storage")).unwrap();

	let dst_f = OpenOptions::new()
		.read(true)
		.write(true)
		.create(true)
		.open(storage_con_base.join("crate_storage_mb")).unwrap();
	let mut src = BlobCrateStorage::new(src_f).unwrap();
	let mut dst = BlobCrateStorage::new(dst_f).unwrap();

	println!("Obtaining graph...");
	let graph = GraphOfBlobs::from_blob_crate_storage(&acj, &mut src);
	println!("root number {}", graph.roots.len());
	dst.store_parallel_mb(&mut src, &graph, 8);
}
