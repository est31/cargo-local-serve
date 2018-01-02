use std::io::Cursor;
use super::blob_storage::{BlobStorage};

#[test]
fn store_and_load() {
	let mut c = Cursor::new(Vec::new());
	let test_data = [
		([1u8; 32], &vec![1,2,3,4,5,6,7,8]),
		([2; 32], &vec![7,8,9,1,1,1,9,8,7]),
		([3; 32], &vec![8,3,8,3,8,3,8,3,8,3,8,3,8,3,8,3,8,3]),
	];
	{
		let mut st = BlobStorage::empty(&mut c);
		for &(d, ref s) in test_data.iter() {
			st.insert(d, s).unwrap();
		}
		st.write_header_and_index().unwrap();
	}
	println!("c {:?}", c);
	{
		let mut st = BlobStorage::load(&mut c).unwrap();
		for &(ref d, s) in test_data.iter() {
			assert_eq!(st.get(d).unwrap().as_ref(), Some(s));
		}
	}
}
