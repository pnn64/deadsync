use codec::*;
use std::io::Write;
fn main() {
 let bytes=std::fs::read(std::env::args().nth(1).unwrap()).unwrap();let mut planes:Vec<Vec<f32>>=Vec::new();let mut end=None;
 let result=decode_reader(bytes.as_slice(),OggOptions{eos_behavior:EosBehavior::Libvorbis,..OggOptions::default()},|e|match e {
 OggEvent::StreamStart(s)=>planes=vec![Vec::new();s.identification.channels as usize],
 OggEvent::Pcm(b)=>{assert_eq!(b.start_frame,planes[0].len() as u64);for(p,q)in planes.iter_mut().zip(b.pcm.planes()){p.extend(q);}},
 OggEvent::StreamEnd(e)=>end=Some(e),_=>{}});
 let mut out=std::io::BufWriter::new(std::io::stdout().lock());out.write_all(&[result.is_ok() as u8]).unwrap();out.write_all(&(planes[0].len() as u64).to_le_bytes()).unwrap();out.write_all(&end.map_or(u64::MAX,|e|e.frames).to_le_bytes()).unwrap();out.write_all(&end.map_or(u64::MAX,|e|e.emitted_frames).to_le_bytes()).unwrap();out.write_all(&(planes.len() as u32).to_le_bytes()).unwrap();for p in planes {for x in p {out.write_all(&x.to_le_bytes()).unwrap();}}
 eprintln!("{result:?}");
}
