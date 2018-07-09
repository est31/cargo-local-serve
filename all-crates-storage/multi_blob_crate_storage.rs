/*!

Embedding multi blob  into blob crate storages

In the first step, we create a directed graph of blobs.
In the second step, we determine minimum spanning trees

*/

use hash_ctx::Digest;
use std::collections::{HashMap, HashSet};
use std::collections::hash_map::Entry;
use std::io::{self, Read, Seek};
use semver::Version;
use petgraph::graph::{Graph, NodeIndex};
use registry::registry::AllCratesJson;
use crate_storage::{CrateSource, CrateSpec};
use blob_crate_storage::BlobCrateStorage;

use super::hash_ctx::HashCtx;

/**
A grah of blobs

Each node represents a blob.
Edges suggest that the two blobs
might be different versions of the same file,
thus possibly have a small diff.
*/
pub struct GraphOfBlobs {
	pub graph :Graph<Digest, ()>,
	pub roots :HashSet<NodeIndex>,
}

macro_rules! optry {
	($e:expr) => {
		match $e {
			Some(d) => d,
			None => return None,
		}
	};
}

impl GraphOfBlobs {
	pub fn from_func(acj :&AllCratesJson,
			mut get_digest_list :impl FnMut(&str, &Version) -> Option<Vec<(Digest, String)>>)
			-> GraphOfBlobs {
		/// Strips the first component of a path
		fn strip_path<'a>(path :&'a str, name :&str, version :&Version) -> &'a str {
			let ver_display_len = format!("{}", version).len();
			let prefix_len =  name.len() + 2 + ver_display_len;
			&path[prefix_len..]
		}
		let mut graph = Graph::new();
		let mut roots = HashSet::new();
		let mut digest_to_node_id = HashMap::new();
		for (_crate_name, crate_versions) in acj {
			let mut path_to_digests = HashMap::new();
			let mut digest_to_version = HashMap::new();
			let mut digest_lists = Vec::new();
			for krate in crate_versions {
				let digest_list = get_digest_list(&krate.name, &krate.version);
				if let Some(digest_list) = digest_list {
					for (digest, path) in digest_list.iter() {
						// TODO find a way to store these long file names
						if path == "././@LongLink" {
							continue;
						}
						let path_stripped = strip_path(path, &krate.name, &krate.version)
							.to_owned();
						let mut digests = path_to_digests.entry(path_stripped)
							.or_insert(HashSet::new());
						digests.insert(*digest);
						digest_to_version.insert(*digest, krate.version.clone());
					}
					digest_lists.push((krate, digest_list));
				}
			}
			// Add the nodes
			for (_krate, digest_list) in digest_lists.iter() {
				for (digest, _path) in digest_list.iter() {
					match digest_to_node_id.entry(*digest) {
						Entry::Occupied(_) => (),
						Entry::Vacant(v) => {
							let id = graph.add_node(*digest);
							roots.insert(id);
							v.insert(id);
						},
					}
				}
			}
			// Add the edges
			for (_path, digests) in path_to_digests.iter() {
				let mut ordered_digests = digests.iter().collect::<Vec<_>>();
				ordered_digests.sort_by_key(|digest| {
					digest_to_version.get(*digest).unwrap()
				});
				let mut node_id_prior = None;
				for digest in ordered_digests {
					let node_id = *digest_to_node_id.get(digest).unwrap();
					if let Some(prior) = node_id_prior {
						roots.remove(&node_id);
						graph.add_edge(prior, node_id, ());
					}
					node_id_prior = Some(node_id);
				}
			}
		}
		GraphOfBlobs {
			graph,
			roots
		}
	}

	pub fn from_crate_source<C :CrateSource>(acj :&AllCratesJson, src :&mut C) -> GraphOfBlobs {
		GraphOfBlobs::from_func(acj, |name :&str, version :&Version| {
			println!("name {} v {}", name, version);
			// TODO instead of obtaining the blobs,
			//      extend the CrateSource trait or the CrateHandle trait
			//      to include a way to obtain this directly.
			//      Some format store the digests along the metadata.
			// TODO don't use unwrap here
			let mut handle = optry!(src.get_crate_handle_nv(name.to_string(), version.clone()));
			let file_list = handle.get_file_list();
			Some(file_list.into_iter()
				.map(|path| {
					let file = handle.get_file(&path).unwrap();
					let mut file_rdr = &*file;
					let mut hash_ctx = HashCtx::new();
					io::copy(&mut file_rdr, &mut hash_ctx).unwrap();
					let digest = hash_ctx.finish_and_get_digest();
					(digest, path)
				})
				.collect::<Vec<_>>())
		})
	}

	pub fn from_blob_crate_storage<S :Read + Seek>(acj :&AllCratesJson,
			src :&mut BlobCrateStorage<S>) -> GraphOfBlobs {
		GraphOfBlobs::from_func(acj, |name :&str, version :&Version| {
			//println!("name {} v {}", name, version);
			let s = CrateSpec {
				name : name.to_string(),
				version : version.clone(),
			};
			// TODO treat LongLink
			let meta = optry!(src.get_crate_rec_meta(&s));
			Some(meta.get_file_digest_list())
		})
	}

}
