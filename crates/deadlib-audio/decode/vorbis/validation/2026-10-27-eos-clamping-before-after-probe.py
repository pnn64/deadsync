from pathlib import Path
import hashlib,json,struct,subprocess,math,shutil
repo=Path.cwd();root=repo/'target/deadlib-audio-reference';source=root/'eos-clamping-20261027';run=root/'eos-clamping-before-after-final-verified-20261027';run.mkdir(exist_ok=False)
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
input=source/'probe-inputs/eos-baseline.ogg';shutil.copyfile(input,run/'input.ogg')
exe=root/'vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe';old=source/'oracle-before.exe'
class Reader:
 def __init__(self,b):self.b=b;self.at=0
 def take(self,n):v=self.b[self.at:self.at+n];self.at+=n;assert len(v)==n;return v
 def num(self,n=8):return int.from_bytes(self.take(n),'little')
 def done(self):assert self.at==len(self.b)
oldpcm=subprocess.run([str(old),'oggpcm',str(run/'input.ogg')],capture_output=True);assert oldpcm.returncode==0
fresh=subprocess.run([str(exe),'oggpcm',str(run/'input.ogg')],capture_output=True);assert fresh.returncode==0 and fresh.stdout==oldpcm.stdout
reference=subprocess.run([str(exe),'oggstream',str(run/'input.ogg')],capture_output=True);assert reference.returncode==0
(run/'reference.out').write_bytes(reference.stdout);(run/'reference-old-mode.out').write_bytes(fresh.stdout)
a=Reader(reference.stdout);channels=a.num(4);rate=a.num(4);expected=[bytearray() for _ in range(channels)];total=0
while True:
 n=a.num(4)
 if not n:break
 assert (a.num(4),a.num(4),a.num(4))==(0,channels,rate);total+=n
 for p in expected:p.extend(a.take(n*4))
a.done();assert total==3648
base=r"""use codec::*;
use std::io::Write;
fn main() {
 let bytes=std::fs::read(std::env::args().nth(1).unwrap()).unwrap();let mut planes:Vec<Vec<f32>>=Vec::new();let mut end=None;
 let result=decode_reader(bytes.as_slice(),OPTIONS,|e|match e {
 OggEvent::StreamStart(s)=>planes=vec![Vec::new();s.identification.channels as usize],
 OggEvent::Pcm(b)=>{assert_eq!(b.start_frame,planes[0].len() as u64);for(p,q)in planes.iter_mut().zip(b.pcm.planes()){p.extend(q);}},
 OggEvent::StreamEnd(e)=>end=Some(e),_=>{}});
 let mut out=std::io::BufWriter::new(std::io::stdout().lock());out.write_all(&[result.is_ok() as u8]).unwrap();out.write_all(&(planes[0].len() as u64).to_le_bytes()).unwrap();out.write_all(&end.map_or(u64::MAX,|e|e.frames).to_le_bytes()).unwrap();out.write_all(&EMITTED.to_le_bytes()).unwrap();out.write_all(&(planes.len() as u32).to_le_bytes()).unwrap();for p in planes {for x in p {out.write_all(&x.to_le_bytes()).unwrap();}}
 eprintln!("{result:?}");
}
"""
rows=[];sources={}
for variant in ['before','after']:
 folder=run/variant;(folder/'src').mkdir(parents=True)
 if variant=='before':
  for package in ['ogg','vorbis']:
   target=folder/package;(target/'src').mkdir(parents=True)
   manifest=(repo/f'crates/deadlib-audio/decode/{package}/Cargo.toml').read_text(encoding='utf8').split('\n[[bench]]')[0];(target/'Cargo.toml').write_text(manifest,encoding='utf8',newline='\n')
   paths=subprocess.check_output(['git','ls-tree','-r','--name-only','f1bc46e6','--',f'crates/deadlib-audio/decode/{package}/src'],text=True).splitlines()
   for name in paths:
    if not name.endswith('.rs'):continue
    blob=subprocess.check_output(['git','show',f'f1bc46e6:{name}']);q=target/'src'/Path(name).name;q.write_bytes(blob);sources[q.relative_to(folder).as_posix()]=sha(q)
  dependency=folder/'vorbis';options='OggOptions::default()';emitted='u64::MAX'
 else:dependency=repo/'crates/deadlib-audio/decode/vorbis';options='OggOptions{eos_behavior:EosBehavior::Libvorbis,..OggOptions::default()}';emitted='end.map_or(u64::MAX,|e|e.emitted_frames)'
 driver=base.replace('OPTIONS',options).replace('EMITTED',emitted);(folder/'src/main.rs').write_text(driver,encoding='utf8',newline='\n');cargo=f'[package]\nname="deadlib-eos-{variant}"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\ncodec={{package="deadlib-audio-decode-vorbis",path={json.dumps(dependency.as_posix())}}}\n';(folder/'Cargo.toml').write_text(cargo,encoding='utf8',newline='\n')
 command=['cargo','build','--offline','--release','--manifest-path',str(folder/'Cargo.toml')]
 with(run/(variant+'-build.log')).open('wb')as f:p=subprocess.run(command,stdout=f,stderr=subprocess.STDOUT)
 assert p.returncode==0,(run/(variant+'-build.log')).read_text(encoding='utf8')
 binary=folder/f'target/release/deadlib-eos-{variant}.exe';p=subprocess.run([str(binary),str(run/'input.ogg')],capture_output=True);assert p.returncode==0,p.stderr;(run/(variant+'.out')).write_bytes(p.stdout);(run/(variant+'.stderr')).write_bytes(p.stderr)
 a=Reader(p.stdout);ok=bool(a.num(1));frames=a.num();declared=a.num();emitted=a.num();assert a.num(4)==channels;assert frames==total
 peak=error=squares=0.;values=0
 for plane in expected:
  for(x,),(y,)in zip(struct.iter_unpack('<f',a.take(frames*4)),struct.iter_unpack('<f',plane)):
   assert math.isfinite(x)and math.isfinite(y);d=abs(x-y);peak=max(peak,abs(y));error=max(error,d);squares+=d*d;values+=1
 a.done();rms=math.sqrt(squares/values);assert error<=3e-6*max(1,peak)and rms<=5e-7*max(1,peak)
 assert ok==(variant=='after');assert declared==(1 if ok else (1<<64)-1);assert emitted==(3648 if ok else (1<<64)-1)
 rows.append({'variant':variant,'command':command,'source_sha256':sha(folder/'src/main.rs'),'manifest_sha256':sha(folder/'Cargo.toml'),'binary_sha256':sha(binary),'output_sha256':sha(run/(variant+'.out')),'success':ok,'frames':frames,'declared':declared,'emitted':emitted,'values':values,'reference_peak':peak,'peak_error':error,'rms_error':rms,'stderr':p.stderr.decode()})
assert (run/'before.out').read_bytes()[29:]==(run/'after.out').read_bytes()[29:]
summary={'scope':'Compiled before/after EOS1 witness. Before rejects after 3648 emitted frames. Explicit reference mode succeeds with declared duration 1 and emitted count 3648. Both PCM prefixes satisfy original bounds. Old and new oracle oggpcm responses identical. No performance measurement.','before_source_commit':'f1bc46e6fdadefa07968a0ac9606989ca288eab1','input_sha256':sha(run/'input.ogg'),'reference_binary_sha256':sha(exe),'old_reference_binary_sha256':sha(old),'reference_response_sha256':sha(run/'reference.out'),'reference_old_mode_sha256':sha(run/'reference-old-mode.out'),'before_sources_sha256':sources,'after_codec_source_sha256':{p.relative_to(repo).as_posix():sha(p) for package in ['ogg','vorbis'] for p in(repo/f'crates/deadlib-audio/decode/{package}/src').glob('*.rs')},'results':rows}
(run/'checks.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf8',newline='\n');print([(x['variant'],x['success'],x['frames'],x['declared'],x['emitted'])for x in rows])
