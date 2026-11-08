use codec::{bitpack::BitReader,codebook::Codebook,Limits};
fn main() {
 let p=std::env::args().nth(1).unwrap();let data=std::fs::read(p).unwrap();let mut bits=BitReader::new(&data);
 let book=Codebook::read(&mut bits,&Limits::default());
 println!("accepted={} consumed={} exhausted={}",book.is_ok(),bits.position(),bits.is_exhausted());
 assert_eq!(book.is_ok(),true);assert_eq!(bits.position(),331);assert!(!bits.is_exhausted());
}
