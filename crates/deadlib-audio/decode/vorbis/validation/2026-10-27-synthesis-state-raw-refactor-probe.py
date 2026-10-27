from pathlib import Path
import hashlib,json,struct,subprocess,shutil
repo=Path.cwd();root=repo/'target/deadlib-audio-reference';run=root/'seek-reference-20261027';out=run/'raw-refactor';out.mkdir(exist_ok=False)
decode=repo/'crates/deadlib-audio/decode';sha=lambda b:hashlib.sha256(b).hexdigest()
commit='921cf04428c6084deea0ab97c40560d0f75008ee'
baseline=json.loads((run/'baseline-source.json').read_text(encoding='utf8'))
for name,h in baseline.items():
 p=run/'baseline-source'/name;assert sha(p.read_bytes())==h
 assert p.read_bytes().replace(b'\r\n',b'\n')==subprocess.check_output(['git','show',commit+':'+name]).replace(b'\r\n',b'\n')
driver=r'''use codec::*;
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
'''
binaries={};builds=[]
for variant in ['before','after']:
 folder=out/variant;(folder/'src').mkdir(parents=True)
 if variant=='before':
  for package in ['ogg','vorbis']:
   target=folder/package;(target/'src').mkdir(parents=True)
   # Keep only the independent production package/dependencies.
   manifest=(decode/package/'Cargo.toml').read_text(encoding='utf8').split('[dev-dependencies]')[0]
   (target/'Cargo.toml').write_text(manifest,encoding='utf8',newline='\n')
   for name in baseline:
    if name.startswith('crates/deadlib-audio/decode/'+package+'/src/'):
     shutil.copyfile(run/'baseline-source'/name,target/'src'/Path(name).name)
  dependency=folder/'vorbis'
 else:dependency=decode/'vorbis'
 (folder/'src/main.rs').write_text(driver,encoding='utf8',newline='\n')
 manifest=f'[package]\nname="deadlib-raw-refactor-{variant}"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\ncodec={{package="deadlib-audio-decode-vorbis",path={json.dumps(dependency.as_posix())}}}\n'
 (folder/'Cargo.toml').write_text(manifest,encoding='utf8',newline='\n')
 command=['cargo','build','--offline','--release','--manifest-path',str(folder/'Cargo.toml')]
 with(out/f'{variant}-build.log').open('wb')as f:r=subprocess.run(command,stdout=f,stderr=subprocess.STDOUT)
 assert r.returncode==0,(out/f'{variant}-build.log').read_text(encoding='utf8')
 binary=folder/f'target/release/deadlib-raw-refactor-{variant}.exe';binaries[variant]=binary
 builds.append({'variant':variant,'command':command,'binary_sha256':sha(binary.read_bytes()),'driver_sha256':sha(driver.encode()),'manifest_sha256':sha(manifest.encode())})
# One complete schedule for every distinct setup/configuration, plus every
# actual vorbisfile schedule. This compares native raw output bits, not C PCM.
inputs=[];seen=set()
for p in sorted((run/'component-inputs').glob('*.bin')):
 q=p.read_bytes();at=0
 for _ in range(3):n=int.from_bytes(q[at:at+8],'little');at+=8+n
 key=sha(q[:at])
 if key in seen:continue
 seen.add(key);inputs.append(p)
inputs += sorted((run/'golden-inputs').glob('*.bin'))
inputs += sorted((run/'captured-final').glob('*/schedule.bin'))
rows=[]
for i,p in enumerate(inputs):
 q=p.read_bytes();responses={}
 for variant,binary in binaries.items():
  r=subprocess.run([str(binary),str(p)],capture_output=True);assert r.returncode==0,(p,variant,r.stderr)
  responses[variant]=r.stdout
 assert responses['before']==responses['after'],p
 rows.append({'request':p.relative_to(repo).as_posix(),'request_sha256':sha(q),'identical_output_sha256':sha(responses['before']),'output_bytes':len(responses['before'])})
 print(i,p.name,len(responses['before']),'identical bytes',flush=True)
summary={'scope':'Compiled native PacketDecoder before/after the unwindowed-time extraction. Identical raw PCM bits, errors, discard decisions, truncation flags, sample counts and positions for every selected schedule. This does not claim vorbisfile seek parity.','before_commit':commit,'baseline_source_sha256':baseline,'after_source_sha256':{p.relative_to(decode).as_posix():sha(p.read_bytes())for package in ['ogg','vorbis']for p in(decode/package/'src').glob('*.rs')},'builds':builds,'requests':rows,'identical_output_bytes':sum(x['output_bytes']for x in rows)}
(out/'checks.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf8',newline='\n')
print(len(rows),'schedules; identical native raw behavior and PCM bits')
