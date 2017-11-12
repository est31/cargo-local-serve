use super::registry::AllCratesJson;
use string_interner::StringInterner;
use semver::{Version, VersionReq};
use std::collections::{HashMap, HashSet};

type CrateName = usize;

pub struct CrateStats {
	pub crate_names_interner :StringInterner<CrateName>,
	pub reverse_dependencies :HashMap<CrateName, HashMap<VersionReq, HashSet<(CrateName, Version)>>>,
	/// The list of crates ordered by the number of crates directly depending on them.
	pub most_directly_depended_on :Vec<(CrateName, usize)>,
}

pub fn compute_crate_statistics(acj :&AllCratesJson) -> CrateStats {
	let mut names_interner = StringInterner::new();
	let mut revd = HashMap::new();
	let mut ddon = HashMap::new(); // TODO populate ddon
	for &(ref name, ref cjv) in acj.iter() {
		let name_i = names_interner.get_or_intern(name.clone());
		for krate in cjv.iter() {
			for dep in krate.dependencies.iter() {
				let dname_i = names_interner.get_or_intern(dep.name.clone());
				let e = revd.entry(dname_i).or_insert(HashMap::new());
				let s = e.entry(dep.req.clone()).or_insert(HashSet::new());
				s.insert((name_i, krate.version.clone()));
			}
		}
	}

	let mut most_directly_depended_on = ddon.into_iter().collect::<Vec<_>>();
	most_directly_depended_on.sort_by_key(|v| v.1);

	CrateStats {
		crate_names_interner : names_interner,
		reverse_dependencies : revd,
		most_directly_depended_on
	}
}
