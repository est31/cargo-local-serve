extern crate all_crate_storage;
extern crate semver;

use semver::Version;

use std::fs::{self, OpenOptions};
use std::env;
use std::str;
use all_crate_storage::registry::registry;
use all_crate_storage::blob_crate_storage::BlobCrateStorage;
use all_crate_storage::crate_storage::{CrateSource};
use all_crate_storage::diff::Diff;
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

	let f = OpenOptions::new()
		.read(true)
		.open(storage_con_base.join("crate_storage")).unwrap();
	let mut cst = BlobCrateStorage::new(f).unwrap();

	let lib_062 = {
		let mut ch = cst.get_crate_handle_nv("lewton".to_owned(),
			Version::parse("0.6.2").unwrap()).unwrap();
		ch.get_file("lewton-0.6.2/src/imdct.rs").unwrap()
	};
	let lib_062_str = str::from_utf8(&lib_062).unwrap();
	let lib_070 = {
		let mut ch = cst.get_crate_handle_nv("lewton".to_owned(),
			Version::parse("0.7.0").unwrap()).unwrap();
		ch.get_file("lewton-0.7.0/src/imdct.rs").unwrap()
	};
	let lib_070_str = str::from_utf8(&lib_070).unwrap();
	let diff = Diff::from_texts(lib_062_str,
		lib_070_str, "\n");

	let mut v = Vec::new();
	diff.serialize(&mut v).unwrap();
	println!("{:?}", diff);
	println!("{} {} {}", lib_062.len(), lib_070.len(), v.len());
	assert_eq!(diff.reconstruct_new(lib_062_str), lib_070_str);
	/*for i in diff.inserts() {
		println!("{:?}", i);
	}
	for d in diff.deletes() {
	}
	println!("{}", lib_062);
	println!("{}", lib_070);*/
}
