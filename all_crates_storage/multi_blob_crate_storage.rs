/*!

Embedding multi blob  into blob crate storages

In the first step, we create a directed graph of blobs.
In the second step, we determine minimum spanning trees

*/

use hash_ctx::Digest;
use super::blob_storage::{write_delim_byte_slice, read_delim_byte_slice};
use std::collections::{HashMap, HashSet};
use std::collections::hash_map::Entry;
use std::io;
use semver::Version;
use petgraph::graph::{Graph, NodeIndex};
use registry::registry::AllCratesJson;
use crate_storage::{CrateSource, CrateSpec};

use super::hash_ctx::HashCtx;
use flate2::{Compression, GzBuilder};
use flate2::read::GzDecoder;
use tar::{Archive, Header};

pub struct GraphOfBlobs {
	pub graph :Graph<Digest, ()>,
	pub roots :HashSet<NodeIndex>,
}

/**
Build a graph of blobs

Build a directed graph of blobs where edges suggest
that exploring whether to create a diff is a good idea or not.
*/
pub fn build_blob_graph_with(acj :&AllCratesJson,
		mut get_digest_list :impl FnMut(&str, &Version) -> Vec<(Digest, String)>)
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
			for (digest, path) in digest_list.iter() {
				let path_stripped = strip_path(path, &krate.name, &krate.version)
					.to_owned();
				let mut digests = path_to_digests.entry(path_stripped)
					.or_insert(HashSet::new());
				digests.insert(*digest);
				digest_to_version.insert(*digest, krate.version.clone());
			}
			digest_lists.push((krate, digest_list));
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

fn get_digest_list_for_crate<R :io::Read>(archive_rdr :R) -> Vec<(Digest, String)> {
	// TODO make this function use try!().
	let gz_dec = GzDecoder::new(archive_rdr);
	let mut archive = Archive::new(gz_dec);
	let mut digest_list = Vec::new();
	for entry in archive.entries().unwrap().raw(true) {
		let mut entry = entry.unwrap();
		let path = {
			let path_bytes = entry.header().path_bytes();
			String::from_utf8(Vec::from(path_bytes)).unwrap()
		};
		let mut hash_ctx = HashCtx::new();
		io::copy(&mut entry, &mut hash_ctx).unwrap();
		let digest = hash_ctx.finish_and_get_digest();
		digest_list.push((digest, path));
	}
	digest_list
}

pub fn build_blob_graph_from_src<C :CrateSource>(acj :&AllCratesJson, src :&mut C) -> GraphOfBlobs {
	build_blob_graph_with(acj, |name :&str, version :&Version| {
		println!("name {} v {}", name, version);
		// TODO instead of obtaining the entire .crate file,
		//      extend the CrateSource trait or the CrateHandle trait
		//      to include a way to obtain this directly.
		//      Some format store the digests along the metadata.
		// TODO don't use unwrap here
		let crate_file = src.get_crate(&CrateSpec {
			name : name.to_string(),
			version : version.clone(),
		}).unwrap();
		get_digest_list_for_crate(&*crate_file)
	})
}
