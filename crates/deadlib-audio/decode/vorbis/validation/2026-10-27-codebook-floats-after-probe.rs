use std::io::Write;
use codec::*;
fn number(wire: &mut &[u8]) -> u64 {let (v,r)=wire.split_at(8);*wire=r;u64::from_le_bytes(v.try_into().unwrap())}
fn packet<'a>(wire:&mut &'a[u8]) -> &'a[u8] {let n=number(wire) as usize;let (v,r)=wire.split_at(n);*wire=r;v}
fn main() {
 let bytes=std::fs::read(std::env::args().nth(1).unwrap()).unwrap();let mut wire=bytes.as_slice();
 let id=Identification::parse(packet(&mut wire)).unwrap();packet(&mut wire);let header=packet(&mut wire);
 let setup=Setup::parse_with_options(header,&id,ParseOptions {codebook_behavior:CodebookBehavior::Libvorbis,..ParseOptions::default()}).unwrap();
 let channels=setup.channels() as u32;
 let mut decoder=PacketDecoder::with_options(setup,DecodeOptions {allow_nonfinite:true,residue2_layout:Residue2Layout::Libvorbis,floor0_behavior:Floor0Behavior::Libvorbis,..DecodeOptions::default()}).unwrap();
 assert_eq!(number(&mut wire),1);let count=number(&mut wire);
 let mut out=std::io::BufWriter::new(std::io::stdout().lock());out.write_all(&channels.to_le_bytes()).unwrap();
 for _ in 0..count {
  match decoder.decode(packet(&mut wire)).unwrap() {
   None=>out.write_all(b"S").unwrap(),
   Some(pcm)=>{
    out.write_all(b"P").unwrap();out.write_all(&(pcm.block_size as u32).to_le_bytes()).unwrap();out.write_all(&(pcm.frames as u32).to_le_bytes()).unwrap();out.write_all(&pcm.start_frame.to_le_bytes()).unwrap();
    for plane in pcm.planes(){for value in plane {out.write_all(&value.to_le_bytes()).unwrap();}}
   }
  }
 }
 assert!(wire.is_empty());
}
