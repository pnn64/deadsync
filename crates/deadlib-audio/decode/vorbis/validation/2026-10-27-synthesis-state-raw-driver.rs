use codec::*;
use std::io::Write;
fn take<'a>(b:&mut &'a [u8],n:usize)->&'a[u8]{let(a,c)=b.split_at(n);*b=c;a}
fn num(b:&mut &[u8])->u64{u64::from_le_bytes(take(b,8).try_into().unwrap())}
fn blob<'a>(b:&mut &'a[u8])->&'a[u8]{let n=num(b)as usize;take(b,n)}
fn main(){
 let input=std::fs::read(std::env::args().nth(1).unwrap()).unwrap();let mut b=input.as_slice();
 let id=Identification::parse(blob(&mut b)).unwrap();blob(&mut b);let header=blob(&mut b);
 let setup=Setup::parse_with_options(header,&id,ParseOptions{allow_empty_codebooks:true,codebook_behavior:CodebookBehavior::Libvorbis,residue_behavior:ResidueBehavior::Libvorbis,..Default::default()}).unwrap();
 let mut d=PacketDecoder::with_options(setup,DecodeOptions{allow_nonfinite:true,floor0_behavior:Floor0Behavior::Libvorbis,floor1_behavior:Floor1Behavior::Libvorbis,residue2_layout:Residue2Layout::Libvorbis,..Default::default()}).unwrap();
 let mut out=std::io::BufWriter::new(std::io::stdout().lock());
 for _ in 0..num(&mut b){d.reset();for _ in 0..num(&mut b){let kind=num(&mut b);out.write_all(&kind.to_le_bytes()).unwrap();match kind{
 0|7=>d.reset(),
 1|2=>{take(&mut b,24);let p=blob(&mut b);match d.decode(p){
 Ok(None)=>out.write_all(&[0]).unwrap(),
 Ok(Some(pcm))=>{out.write_all(&[1]).unwrap();out.write_all(&pcm.start_frame.to_le_bytes()).unwrap();for n in [pcm.channels,pcm.frames,pcm.block_size]{out.write_all(&(n as u32).to_le_bytes()).unwrap();}out.write_all(&[pcm.truncated as u8]).unwrap();for plane in pcm.planes(){for x in plane{out.write_all(&x.to_bits().to_le_bytes()).unwrap();}}},
 Err(e)=>{let s=format!("{e:?}");out.write_all(&[2]).unwrap();out.write_all(&(s.len()as u64).to_le_bytes()).unwrap();out.write_all(s.as_bytes()).unwrap();}}},
 3=>{take(&mut b,8);},4=>{take(&mut b,24);},5=>{},6=>{let large=num(&mut b)as usize;take(&mut b,24);take(&mut b,id.channels as usize*id.block_sizes[large]*8);},_=>panic!()}}
 }assert!(b.is_empty());}
