use std::io::Result as IoResult;
use std::io::{Read, Write};
use difference::{Difference, Changeset};
use byteorder::{ReadBytesExt, WriteBytesExt, BigEndian};
use super::blob_storage::{write_delim_byte_slice, read_delim_byte_slice};
use std::str;

#[derive(Debug)]
pub enum DiffInstruction {
	Same(u64),
	Addition(String),
	Removal(u64),
}

impl DiffInstruction {
	fn from_difference_last(d :Difference) -> Self {
		use self::Difference as D;
		use self::DiffInstruction as Di;
		match d {
			D::Same(s) => Di::Same(s.len() as u64),
			D::Add(s) => Di::Addition(s),
			D::Rem(s) => Di::Removal(s.len() as u64),
		}
	}
	fn from_difference(d :Difference, sep :&str) -> Self {
		use self::Difference as D;
		use self::DiffInstruction as Di;

		match d {
			D::Same(s) => Di::Same((s.len() + sep.len()) as u64),
			D::Add(s) => Di::Addition(s + "\n"),
			D::Rem(s) => Di::Removal((s.len() + sep.len()) as u64),
		}
	}
}

#[derive(Debug)]
pub struct Diff {
	instructions :Vec<DiffInstruction>,
}

impl Diff {
	pub fn from_texts_nl(old :&str, new :&str) -> Self {
		Diff::from_texts(old, new, "\n")
	}
	pub fn from_texts(old :&str, new :&str, sep :&str) -> Self {
		let cset = Changeset::new(old, new, sep);
		let len = cset.diffs.len();
		let instructions = cset.diffs.into_iter()
			.enumerate()
			.map(|(i, d)| {
				println!("{:?}", d);
				if i + 1 < len {
					DiffInstruction::from_difference(d, sep)
				} else {
					DiffInstruction::from_difference_last(d)
				}
			})
			.collect::<Vec<_>>();
		Diff {
			instructions,
		}
	}
	pub fn reconstruct_new(&self, old :&str) -> String {
		let mut res = String::new();
		let mut oldsl = old;
		for ins in self.instructions.iter() {
			match ins {
				&DiffInstruction::Same(l) => {
					let adv = l as usize;
					res += &oldsl[..adv];
					oldsl = &oldsl[adv..];
				},
				&DiffInstruction::Addition(ref s) => {
					res += s;
				},
				&DiffInstruction::Removal(l) => {
					let adv = l as usize;
					oldsl = &oldsl[adv..];
				},
			}
		}
		res
	}
	pub fn deserialize<R :Read>(mut rdr :R) -> IoResult<Self> {
		let len = try!(rdr.read_u64::<BigEndian>());
		let mut instructions = Vec::with_capacity(len as usize);
		for _ in 0 .. len {
			let kind = try!(rdr.read_u8());
			let ins = match kind {
				1 => DiffInstruction::Same(try!(rdr.read_u64::<BigEndian>())),
				2 => {
					let sl = try!(read_delim_byte_slice(&mut rdr));
					// TODO get rid of unwrap here
					let s = str::from_utf8(&sl).unwrap();
					DiffInstruction::Addition(s.to_owned())
				},
				3 => DiffInstruction::Removal(try!(rdr.read_u64::<BigEndian>())),
				_ => panic!("Invalid kind {}", kind),
			};
			instructions.push(ins);
		}
		Ok(Diff {
			instructions
		})
	}
	pub fn serialize<W :Write>(&self, mut wtr :W) -> IoResult<()> {
		try!(wtr.write_u64::<BigEndian>(self.instructions.len() as u64));
		for ins in self.instructions.iter() {
			let kind = match ins {
				&DiffInstruction::Same(_) => 1,
				&DiffInstruction::Addition(_) => 2,
				&DiffInstruction::Removal(_) => 3,
			};
			try!(wtr.write_u8(kind));
			match ins {
				&DiffInstruction::Same(l) => try!(wtr.write_u64::<BigEndian>(l)),
				&DiffInstruction::Addition(ref s) => {
					try!(write_delim_byte_slice(&mut wtr, s.as_bytes()));
				},
				&DiffInstruction::Removal(l) => try!(wtr.write_u64::<BigEndian>(l)),
			}
		}
		Ok(())
	}
}

#[cfg(test)]
mod test {
	use super::*;
	#[test]
	fn test_simple() {
		let str_a = r#"
			Hello, dear
			World!
			Nice to see you."#;
		let str_b = r#"
			Hello, dear
			Reader!
			Nice to see you."#;
		let diff = Diff::from_texts_nl(str_a, str_b);
		let str_b_reconstructed = diff.reconstruct_new(str_a);
		assert_eq!(str_b, str_b_reconstructed);
	}
	#[test]
	fn test_simple_ser_de() {
		let str_a = r#"
			Hello, dear
			World!
			Nice to see you."#;
		let str_b = r#"
			Hello, dear
			Reader!
			Nice to see you."#;
		let diff = Diff::from_texts_nl(str_a, str_b);
		let mut v = Vec::new();
		diff.serialize(&mut v).unwrap();
		let diff_reconstructed = Diff::deserialize(v.as_slice()).unwrap();
		let str_b_reconstructed = diff_reconstructed.reconstruct_new(str_a);
		assert_eq!(str_b, str_b_reconstructed);
	}
	#[test]
	fn test_another() {
		let str_a = r#"
			For this test,
			we wonder about the first line."#;
		let str_b = r#"
			For this example,
			we wonder about the first line."#;
		let diff = Diff::from_texts_nl(str_a, str_b);
		let str_b_reconstructed = diff.reconstruct_new(str_a);
		assert_eq!(str_b, str_b_reconstructed);
	}
}
