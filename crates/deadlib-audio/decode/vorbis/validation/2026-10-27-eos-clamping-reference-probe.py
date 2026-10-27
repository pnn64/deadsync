from pathlib import Path
import hashlib,json,struct,subprocess,sys,time
repo=Path.cwd();root=repo/'target/deadlib-audio-reference';run=root/'eos-clamping-20261027';gates=root/'eos-clamping-validation-final-verified-20261027';fixtures=repo/'crates/deadlib-audio/decode/vorbis/tests/fixtures'
exe=root/'vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe';old=run/'oracle-before.exe';sha=lambda b:hashlib.sha256(b).hexdigest()
class Reader:
 def __init__(self,b):self.b=b;self.at=0
 def take(self,n):v=self.b[self.at:self.at+n];self.at+=n;assert len(v)==n;return v
 def num(self,n=8):return int.from_bytes(self.take(n),'little')
 def packet(self):return self.take(self.num())
 def done(self):assert self.at==len(self.b)
def packed(parts):return b''.join(struct.pack('<Q',len(p))+p for p in parts)
def oracle(mode,b,binary=exe,requests=None):
 p=run/'reference-input.bin';p.write_bytes(b);args=[str(binary),mode,str(p)]
 if requests is not None:q=run/'reference-seeks.bin';q.write_bytes(requests);args.append(str(q))
 r=subprocess.run(args,capture_output=True);assert r.returncode==0,(mode,r.stderr);return r.stdout
def stream(raw):
 a=Reader(raw);ch=a.num(4);rate=a.num(4);planes=[bytearray()for _ in range(ch)]
 while True:
  n=a.num(4)
  if not n:break
  assert(a.num(4),a.num(4),a.num(4))==(0,ch,rate)
  for p in planes:p.extend(a.take(n*4))
 a.done();return planes
packet_goldens=[];a=Reader((fixtures/'eos-packets-v1.libvorbis').read_bytes());assert a.take(8)==b'DEOSPK01' and a.num(4)==2
for mode in ['pcm-timed-cases','eos-state-cases']:
 q=a.packet();r=a.packet();assert oracle(mode,q)==r;packet_goldens.append({'mode':mode,'request_sha256':sha(q),'response_sha256':sha(r)})
a.done();stream_goldens=[];a=Reader((fixtures/'eos-stream-v1.libvorbis-pcm').read_bytes());assert a.take(8)==b'DEOSOG01';count=a.num(4)
for i in range(count):
 b=a.packet();expected=a.packet();declared=a.num();out=oracle('oggstream',b);assert out==expected;request=struct.pack('<QQQ',1,0,1);seek=oracle('oggseekstates',b,requests=request);assert int.from_bytes(seek[16:24],'little')==declared
 stream_goldens.append({'case':i,'ogg_sha256':sha(b),'stream_response_sha256':sha(out),'declared':declared,'emitted':len(stream(out)[0])//4})
a.done();assert count==25
timed=[]
for fixture in range(4):
 p=gates/'eos-inputs'/f'eos-timed-{fixture}.bin';q=p.read_bytes();out=oracle('pcm-timed-cases',q);assert out==p.with_suffix('.out').read_bytes();timed.append({'fixture':fixture,'request_sha256':sha(q),'response_sha256':sha(out)})
p=gates/'eos-inputs/eos-states.bin';q=p.read_bytes();out=oracle('eos-state-cases',q);assert out==p.with_suffix('.out').read_bytes();a=Reader(q);headers=[a.packet()for _ in range(6)];count=a.num();singles=[];batch=Reader(out);width=batch.num(4);channels=batch.num(4);assert batch.num()==count
for i in range(count):
 payload=a.take(24);b=packed(headers)+struct.pack('<Q',1)+payload;r=oracle('eos-state-cases',b);start=batch.at;block=batch.num(4);frames=batch.num(4);batch.take(16+frames*channels*4);assert r==struct.pack('<IIQ',width,channels,1)+out[start:batch.at];singles.append({'case':i,'known':int.from_bytes(payload[:8],'little'),'state':int.from_bytes(payload[8:16],'little'),'granule':int.from_bytes(payload[16:24],'little'),'request_sha256':sha(b),'response_sha256':sha(r)})
a.done();batch.done();assert count==64
compatibility=[]
for directory,subdir,prefix in [('degenerate-books-20261027','golden-inputs','book-golden-'),('residue-geometry-20261027','golden-inputs','geometry-golden-'),('floor0-parameters-20261027','golden-pcm-inputs','floor0-golden-'),('codebook-floats-20261027','golden-pcm-inputs','float-golden-'),('residue-tails-20261027','golden-pcm-inputs','residue-tail-golden-'),('floor1-guards-20261027','golden-pcm-inputs','floor1-golden-')]:
 for p in sorted((root/directory/subdir).glob(prefix+'*.bin')):
  q=p.read_bytes();out=oracle('pcm-cases',q);assert out==oracle('pcm-cases',q,old);compatibility.append({'request':p.relative_to(repo).as_posix(),'request_sha256':sha(q),'response_sha256':sha(out)})
assert len(compatibility)==40
if '--wait' in sys.argv:
 print('Fresh goldens, callbacks and 40 prior PCM responses match. Waiting for the complete procedural matrix.',flush=True)
 while sum(1 for p in(gates/'configuration-inputs').glob('*.bin'))<18737:time.sleep(5)
unchanged=[]
for p in sorted((root/'floor1-guards-validation-verified-20261027/configuration-inputs').glob('*.bin')):
 q=gates/'configuration-inputs'/p.name;assert q.exists()and q.read_bytes()==p.read_bytes(),p.name;unchanged.append({'name':p.name,'sha256':sha(p.read_bytes())})
assert len(unchanged)==18737
raw=[]
for name in ['stereo','mono','surround','eight-channel']:
 b=(fixtures/(name+'.ogg')).read_bytes();at=0;pending=bytearray();packets=[]
 while at<len(b):
  n=b[at+26];laces=b[at+27:at+27+n];body=at+27+n
  for size in laces:
   pending.extend(b[body:body+size]);body+=size
   if size<255:packets.append(bytes(pending));pending.clear()
  at=body
 assert not pending;r=Reader(oracle('pcm',packed(packets)));ch=r.num(4);planes=[bytearray()for _ in range(ch)]
 for _ in packets[3:]:
  assert r.take(1)==b'P';block=r.num(4);frames=r.num(4)
  for p in planes:p.extend(r.take(frames*4))
 r.done();raw.append(planes)
seedroot=root/'eos-clamping-seeds-verified-20261027';seeds=json.loads((seedroot/'targeted.json').read_text(encoding='utf8'));assert len(seeds['probes'])==1260 and len(seeds['seeds'])==5040 and seeds['reference_binary_sha256']==sha(exe.read_bytes());prefixes=[]
for row in seeds['probes']:
 i=row['case'];p=repo/row['source'];b=p.read_bytes();assert sha(b)==row['ogg_sha256'];out=p.with_suffix('.out').read_bytes();assert sha(out)==row['reference_response_sha256'];planes=stream(out);original=raw[i//315];assert all(p==q[:len(p)]for p,q in zip(planes,original));prefixes.append({'case':i,'frames':len(planes[0])//4,'raw_reference_prefix_identical':True})
zero=(gates/'eos-inputs/eos-stream-180.ogg').read_bytes();new=oracle('oggstream',zero);seek=oracle('oggseekstates',zero,requests=struct.pack('<QQQ',1,0,100000));oldmode=oracle('oggpcm',zero);assert oldmode==oracle('oggpcm',zero,old)and len(oldmode)==24;assert len(stream(new)[0])//4==3648 and int.from_bytes(seek[16:24],'little')==0
z=Reader(seek);z.take(24);assert z.num(4)==0 and z.num()==0;zero_frames=0
while True:
 n=z.num(4)
 if not n:break
 assert z.num(4)==0;zero_frames+=n;z.take(n*8)
z.done();assert zero_frames==3648
inventory=(gates/'eos-inputs/known-seek-differences.tsv').read_bytes();assert inventory==(fixtures/'eos-seek-differences-v1.tsv').read_bytes();rows=[list(map(int,line.split('\t')))for line in inventory.decode().splitlines()];assert len(rows)==172
summary={'scope':'Fresh timed/boundary and offline golden recaptures; all 1260 C stream PCM results equal independent raw C prefixes. Seed preparation freshly recaptured every stream. All earlier procedural inputs and 40 PCM helper responses unchanged. The 172 seek differences remain explicitly unresolved.','reference_binary_sha256':sha(exe.read_bytes()),'old_reference_binary_sha256':sha(old.read_bytes()),'packet_goldens':packet_goldens,'stream_goldens':stream_goldens,'timed_batches':timed,'fresh_boundary_records':singles,'unchanged_procedural_inputs':unchanged,'old_pcm_responses':compatibility,'raw_prefix_checks':prefixes,'known_seek_differences':{'requests':4476,'differences':172,'position_differences':sum(r[3]!=r[5]for r in rows),'length_differences':sum(r[4]!=r[6]for r in rows),'inventory_sha256':sha(inventory)},'zero_total_oracle_regression':{'declared':0,'stream_emitted':3648,'old_shortcut_response_bytes':24,'ogg_sha256':sha(zero),'stream_response_sha256':sha(new),'seek_response_sha256':sha(seek)}}
(run/'independent-reference.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf8',newline='\n');print('Fresh references verified;',len(unchanged),'unchanged requests;',len(compatibility),'old PCM responses;',len(prefixes),'raw C prefixes;172 open seek differences.')
