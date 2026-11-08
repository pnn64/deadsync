use codec::*;
use std::io::{Read, Write};
fn take<'a>(b:&mut &'a[u8],n:usize)->&'a[u8]{let(a,c)=b.split_at(n);*b=c;a}
fn num(b:&mut &[u8])->u64{u64::from_le_bytes(take(b,8).try_into().unwrap())}
fn blob<'a>(b:&mut &'a[u8])->&'a[u8]{let n=num(b)as usize;take(b,n)}
fn main(){
 let q=std::fs::read(std::env::args().nth(1).unwrap()).unwrap();let mut b=q.as_slice();let id=Identification::parse(blob(&mut b)).unwrap();blob(&mut b);let setup=Setup::parse_with_options(blob(&mut b),&id,ParseOptions{allow_empty_codebooks:true,codebook_behavior:CodebookBehavior::Libvorbis,residue_behavior:ResidueBehavior::Libvorbis,..Default::default()}).unwrap();
 let mut out=std::io::BufWriter::new(std::io::stdout().lock());out.write_all(&(setup.modes().len()as u32).to_le_bytes()).unwrap();for m in setup.modes(){out.write_all(&[m.long_block as u8]).unwrap();}out.flush().unwrap();
 let mut d=SynthesisDecoder::with_options(setup,DecodeOptions{allow_nonfinite:true,floor0_behavior:Floor0Behavior::Libvorbis,floor1_behavior:Floor1Behavior::Libvorbis,residue2_layout:Residue2Layout::Libvorbis,..Default::default()}).unwrap();let mut input=std::io::stdin().lock();
 fn n(input:&mut impl Read)->u64{let mut b=[0;8];input.read_exact(&mut b).unwrap();u64::from_le_bytes(b)}
 loop{let mut kind=[0];if input.read_exact(&mut kind).is_err(){break;}let result=match kind[0]{
 0=>{d.reset();Ok(true)},
 1|2=>{let t=PacketTiming{packet_number:n(&mut input),granule_position:Some(n(&mut input)as i64),end_of_stream:n(&mut input)!=0};let mut p=vec![0;n(&mut input)as usize];input.read_exact(&mut p).unwrap();if kind[0]==1{d.submit(&p,t)}else{d.track(&p,t)}},
 3=>d.consume(n(&mut input)as usize).map(|_|true),_=>panic!()};
 let status=match result{Ok(true)=>1,Ok(false)=>0,Err(ref e)=>{eprintln!("{e:?}");2}};out.write_all(&[status]).unwrap();let pcm=d.pcm();out.write_all(&(pcm.frames as u32).to_le_bytes()).unwrap();for plane in pcm.planes(){for x in plane{out.write_all(&x.to_bits().to_le_bytes()).unwrap();}}out.flush().unwrap();
 }
}
