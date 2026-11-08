from pathlib import Path
import hashlib,json,struct,subprocess,math,shutil
repo=Path.cwd();root=repo/'target/deadlib-audio-reference';source=root/'codebook-floats-20261027';run=root/'codebook-floats-before-after-verified-20261027';run.mkdir(exist_ok=False)
seeds=root/'codebook-floats-seeds-verified-20261027';meta=json.loads((seeds/'targeted.json').read_text(encoding='utf8'));case=next(x for x in meta['probes'] if x['label']=='exponent-852-sign-0-role-minimum-mantissa-1');raw=(seeds/f"probe-{case['case']}.bin").read_bytes();(run/'input.bin').write_bytes(raw)
exe=root/'vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe';old=source/'oracle-before.exe';sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def oracle(binary):
 r=subprocess.run([str(binary),'pcm-cases',str(run/'input.bin')],capture_output=True);assert r.returncode==0,r.stderr;return r.stdout
reference=oracle(exe);assert reference==oracle(old);(run/'reference.out').write_bytes(reference)
class Reader:
 def __init__(self,b):self.b=b;self.at=0
 def take(self,n):v=self.b[self.at:self.at+n];self.at+=n;assert len(v)==n;return v
 def num(self,n=8):return int.from_bytes(self.take(n),'little')
 def done(self):assert self.at==len(self.b)
base=r"""use std::io::Write;
use codec::*;
fn number(wire: &mut &[u8]) -> u64 {let (v,r)=wire.split_at(8);*wire=r;u64::from_le_bytes(v.try_into().unwrap())}
fn packet<'a>(wire:&mut &'a[u8]) -> &'a[u8] {let n=number(wire) as usize;let (v,r)=wire.split_at(n);*wire=r;v}
fn main() {
 let bytes=std::fs::read(std::env::args().nth(1).unwrap()).unwrap();let mut wire=bytes.as_slice();
 let id=Identification::parse(packet(&mut wire)).unwrap();packet(&mut wire);let header=packet(&mut wire);
 let setup=Setup::parse_with_options(header,&id,PARSE_OPTIONS).unwrap();
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
"""
results=[]
for variant in ['before','after']:
 folder=run/variant;(folder/'src').mkdir(parents=True)
 if variant=='before':
  for package in ['ogg','vorbis']:
   target=folder/package;(target/'src').mkdir(parents=True)
   manifest=(repo/f'crates/deadlib-audio/decode/{package}/Cargo.toml').read_text(encoding='utf8').split('\n[[bench]]')[0]
   (target/'Cargo.toml').write_text(manifest,encoding='utf8',newline='\n')
   for p in (repo/f'crates/deadlib-audio/decode/{package}/src').glob('*.rs'):
    q=source/'baseline-source'/p.relative_to(repo) if package=='vorbis' else p
    shutil.copyfile(q,target/'src'/p.name)
  dependency=folder/'vorbis';options='ParseOptions::default()'
 else:dependency=repo/'crates/deadlib-audio/decode/vorbis';options='ParseOptions {codebook_behavior:CodebookBehavior::Libvorbis,..ParseOptions::default()}'
 driver=base.replace('PARSE_OPTIONS',options);(folder/'src/main.rs').write_text(driver,encoding='utf8',newline='\n')
 cargo=f'[package]\nname="deadlib-float-{variant}"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\ncodec={{package="deadlib-audio-decode-vorbis",path={json.dumps(dependency.as_posix())}}}\n';(folder/'Cargo.toml').write_text(cargo,encoding='utf8',newline='\n')
 command=['cargo','build','--offline','--release','--manifest-path',str(folder/'Cargo.toml')]
 with (run/(variant+'-build.log')).open('wb') as f:r=subprocess.run(command,stdout=f,stderr=subprocess.STDOUT)
 assert r.returncode==0,(variant,(run/(variant+'-build.log')).read_text(encoding='utf8',errors='replace'))
 binary=folder/f'target/release/deadlib-float-{variant}.exe';r=subprocess.run([str(binary),str(run/'input.bin')],capture_output=True);assert r.returncode==0,r.stderr;(run/(variant+'.out')).write_bytes(r.stdout)
 a=Reader(reference);b=Reader(r.stdout);channels=a.num(4);assert channels==b.num(4) and a.num()==1
 peak=0.;error=0.;squares=0.;samples=0;position=0;blocks=[]
 for _ in range(5):
  kind=a.take(1);assert kind==b.take(1)==b'P';n=a.num(4);frames=a.num(4);assert n==b.num(4) and frames==b.num(4) and b.num()==position
  block_peak=0.;block_error=0.;block_squares=0.;block_samples=frames*channels
  for _ in range(block_samples):
   expected=struct.unpack('<f',a.take(4))[0];actual=struct.unpack('<f',b.take(4))[0];assert math.isfinite(expected) and math.isfinite(actual)
   d=abs(actual-expected);block_peak=max(block_peak,abs(expected));block_error=max(block_error,d);block_squares+=d*d
  if variant=='after' and block_samples:assert block_error<=3e-6*max(1,block_peak) and math.sqrt(block_squares/block_samples)<=5e-7*max(1,block_peak)
  blocks.append({'block_size':n,'start_frame':position,'frames':frames,'finite_values':block_samples,'peak_error':block_error,'reference_peak':block_peak})
  peak=max(peak,block_peak);error=max(error,block_error);squares+=block_squares;samples+=block_samples;position+=frames
 a.done();b.done();assert position==368 and samples==736
 results.append({'variant':variant,'command':command,'source_sha256':sha(folder/'src/main.rs'),'manifest_sha256':sha(folder/'Cargo.toml'),'binary_sha256':sha(binary),'pcm_sha256':sha(run/(variant+'.out')),'frames':position,'finite_values':samples,'peak_error':error,'rms_error':math.sqrt(squares/samples),'reference_peak':peak,'matches_original_pcm_bounds':variant=='after','blocks':blocks})
assert results[0]['peak_error']>results[0]['reference_peak']*.9
before_sources={p.relative_to(run/'before').as_posix():sha(p) for p in (run/'before').rglob('*.rs') if 'target' not in p.relative_to(run/'before').parts}
summary={'scope':'Before/after finite PCM witness at exponent field 852 and mantissa one. Before is committed b5953ce15 with published defaults; after explicitly selects reference codebook behavior. Default published semantics remain separately tested. Counts/positions agree; only the reference selection satisfies the original PCM bounds. No timing run.','before_source_commit':'b5953ce158c79d75d3cfae5bb62333fbab00e804','input_sha256':sha(run/'input.bin'),'reference_response_sha256':sha(run/'reference.out'),'reference_binary_sha256':sha(exe),'old_reference_binary_sha256':sha(old),'reference_helpers_byte_identical':True,'before_sources_sha256':before_sources,'after_codec_source_sha256':{p.relative_to(repo).as_posix():sha(p) for package in ['ogg','vorbis'] for p in (repo/f'crates/deadlib-audio/decode/{package}/src').glob('*.rs')},'results':results}
(run/'checks.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf8',newline='\n')
print([(x['variant'],x['frames'],x['peak_error']/x['reference_peak'],x['rms_error']/x['reference_peak']) for x in results])
