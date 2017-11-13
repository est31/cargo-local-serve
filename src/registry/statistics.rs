use super::registry::AllCratesJson;
use string_interner::StringInterner;
use semver::{Version, VersionReq};
use std::collections::{HashMap, HashSet};

type CrateName = usize;

pub struct CrateStats {
	pub crate_names_interner :StringInterner<CrateName>,
	/// Mapping a crate to its latest version
	pub latest_crate_versions :HashMap<CrateName, Version>,
	/// Mapping a crate to its reverse dependencies
	pub reverse_dependencies :HashMap<CrateName, HashMap<VersionReq, HashSet<(CrateName, Version)>>>,
	/// The list of crates ordered by the number of crates directly depending on them.
	///
	/// The algorithm doesn't count any reverse dependency where only
	/// a past version depended on a crate, but not the latest one.
	pub most_directly_depended_on :Vec<(CrateName, usize)>,
}

pub fn compute_crate_statistics(acj :&AllCratesJson) -> CrateStats {
	let mut names_interner = StringInterner::new();

	let mut latest_crate_versions = HashMap::new();
	for &(ref name, ref cjv) in acj.iter() {
		let name_i = names_interner.get_or_intern(name.clone());
		if let Some(newest_krate) = cjv.iter().max_by_key(|krate| &krate.version) {
			latest_crate_versions.insert(name_i, newest_krate.version.clone());
		}
	}

	let mut revd = HashMap::new();
	let mut ddon = HashMap::<CrateName, HashSet<CrateName>>::new();
	for &(ref name, ref cjv) in acj.iter() {
		let name_i = names_interner.get_or_intern(name.clone());
		let latest_version = latest_crate_versions.get(&name_i).unwrap();
		for krate in cjv.iter() {
			for dep in krate.dependencies.iter() {
				let dname_i = names_interner.get_or_intern(dep.name.clone());
				let e = revd.entry(dname_i).or_insert(HashMap::new());
				let s = e.entry(dep.req.clone()).or_insert(HashSet::new());
				s.insert((name_i, krate.version.clone()));
				if &krate.version == latest_version {
					let s = ddon.entry(dname_i).or_insert(HashSet::new());
					s.insert(name_i);
				}
			}
		}
	}

	let mut most_directly_depended_on = ddon.into_iter()
		.map(|(n, s)| (n, s.len()))
		.collect::<Vec<_>>();
	most_directly_depended_on.sort_by_key(|v| v.1);

	CrateStats {
		crate_names_interner : names_interner,
		reverse_dependencies : revd,
		latest_crate_versions,
		most_directly_depended_on,
	}
}
