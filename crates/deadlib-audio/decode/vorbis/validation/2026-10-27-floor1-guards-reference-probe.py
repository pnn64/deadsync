from pathlib import Path
import hashlib,json,struct,subprocess
repo=Path.cwd();root=repo/'target/deadlib-audio-reference';run=root/'floor1-guards-20261027';gates=root/'floor1-guards-validation-verified-20261027'
exe=root/'vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe';old=run/'oracle-before.exe';sha=lambda b:hashlib.sha256(b).hexdigest()
class Reader:
 def __init__(self,b):self.b=b;self.at=0
 def take(self,n):v=self.b[self.at:self.at+n];self.at+=n;assert len(v)==n;return v
 def num(self,n=8):return int.from_bytes(self.take(n),'little')
 def packet(self):return self.take(self.num())
 def done(self):assert self.at==len(self.b)
def packed(items):return b''.join(struct.pack('<Q',len(b))+b for b in items)
def oracle(mode,raw,binary=exe):
 p=run/'independent-input.bin';p.write_bytes(raw);r=subprocess.run([str(binary),mode,str(p)],capture_output=True);assert r.returncode==0,(mode,r.stderr);return r.stdout
fixture=repo/'crates/deadlib-audio/decode/vorbis/tests/fixtures'
a=Reader((fixture/'floor1-components-v1.libvorbis').read_bytes());assert a.take(8)==b'DF1CMP01';component_goldens=[]
for i in range(a.num(4)):
 raw=a.packet();expected=a.packet();out=oracle('floor1cases',raw);assert out==expected
 component_goldens.append({'case':i,'request_sha256':sha(raw),'response_sha256':sha(out)})
a.done();assert len(component_goldens)==8
matrix=[];singles=[];selected=set();case_count=value_count=0
for p in sorted((run/'component-inputs').glob('floor1-component-*-batch-*.bin')):
 raw=p.read_bytes();out=oracle('floor1cases',raw);assert out==p.with_suffix('.out').read_bytes()
 matrix.append({'request':p.relative_to(repo).as_posix(),'request_sha256':sha(raw),'response_sha256':sha(out)})
 a=Reader(raw);b=Reader(out);headers=[a.packet() for _ in range(3)];header_sha=sha(packed(headers));count=a.num();assert count==b.num()
 for i in range(count):
  start=a.at;long=a.num();prefix=a.num();sticky=a.num();payload=a.packet();end=a.at
  startout=b.at;present=b.num(1);position=b.num();eof=b.num(1);bins=b.num(4);curve=b.take(bins*4);expected=struct.pack('<Q',1)+out[startout:b.at]
  words=[w for (w,) in struct.iter_unpack('<I',curve)];corner=(not any(words),0 in words,0x3f800000 in words,len(set(words))==1)
  key=(header_sha,long,prefix,sticky,present,eof,corner)
  if key not in selected:
   request=packed(headers)+struct.pack('<Q',1)+raw[start:end];fresh=oracle('floor1cases',request);assert fresh==expected
   selected.add(key);singles.append({'header_sha256':header_sha,'long_block':bool(long),'prefix_bits':prefix,'sticky_input':bool(sticky),'present':bool(present),'sticky_output':bool(eof),'curve_corner_flags':corner,'position':position,'request_sha256':sha(request),'response_sha256':sha(fresh)})
  case_count+=1;value_count+=bins
 a.done();b.done()
assert case_count==83876 and value_count==6710080
previous=root/'residue-tails-validation-verified-20261027/configuration-inputs';unchanged=[]
for p in sorted(previous.glob('*.bin')):
 q=gates/'configuration-inputs'/p.name;assert q.exists() and p.read_bytes()==q.read_bytes(),p.name
 unchanged.append({'name':p.name,'sha256':sha(p.read_bytes())})
assert len(unchanged)==18077
compatibility=[]
for directory,subdir,prefix in [('degenerate-books-20261027','golden-inputs','book-golden-'),('residue-geometry-20261027','golden-inputs','geometry-golden-'),('floor0-parameters-20261027','golden-pcm-inputs','floor0-golden-'),('codebook-floats-20261027','golden-pcm-inputs','float-golden-'),('residue-tails-20261027','golden-pcm-inputs','residue-tail-golden-')]:
 for p in sorted((root/directory/subdir).glob(prefix+'*.bin')):
  raw=p.read_bytes();out=oracle('pcm-cases',raw);assert out==oracle('pcm-cases',raw,old)
  compatibility.append({'request':p.relative_to(repo).as_posix(),'request_sha256':sha(raw),'response_sha256':sha(out)})
assert len(compatibility)==34
def canonical(plane):return b''.join(struct.pack('<I',0x7fc00000 if w&0x7fffffff>0x7f800000 else w) for (w,) in struct.iter_unpack('<I',plane))
a=Reader((fixture/'floor1-pcm-v1.libvorbis-pcm').read_bytes());assert a.take(8)==b'DF1PCM01' and a.num(4)==6;container=[];pcm=[]
for i in range(6):
 raw=(run/'golden-pcm-inputs'/f'floor1-golden-{i}.bin').read_bytes();expected=a.packet();out=oracle('pcm-cases',raw);assert out==expected==oracle('pcm-cases',raw,old)
 r=Reader(raw);headers=[r.packet() for _ in range(3)];assert r.num()==1;packets=[r.packet() for _ in range(r.num())];r.done()
 assert oracle('pcm',packed(headers+packets))==out[:4]+out[12:]
 b=Reader(out);channels=b.num(4);assert b.num()==1;planes=[bytearray() for _ in range(channels)];ends=[0]
 for _ in packets:
  assert b.take(1)==b'P';block=b.num(4);frames=b.num(4);ends.append(ends[-1]+frames)
  for plane in planes:plane.extend(b.take(frames*4))
 b.done();planes=list(map(canonical,planes));total=ends[-1];assert total==368
 data=(gates/'floor1-inputs'/f'floor1-stream-{i}.ogg').read_bytes();path=run/'independent-stream.ogg';path.write_bytes(data)
 result=subprocess.run([str(exe),'oggpcm',str(path)],capture_output=True);assert result.returncode==0,result.stderr
 b=Reader(result.stdout);assert (b.num(4),b.num(4),b.num(4),b.num(4),b.num())==(1,97,channels,48000,total)
 got=[bytearray() for _ in planes];frames_total=0
 while b.at<len(b.b):
  assert b.num(4)==0;frames=b.num(4);frames_total+=frames
  for plane in got:plane.extend(b.take(frames*4))
 assert frames_total==total and list(map(canonical,got))==planes
 targets=[0,1,total-1,total,0]+[v for e in ends[2:-1] for v in [e-1,e,e+1]];q=run/'independent-seeks.bin';q.write_bytes(struct.pack('<Q',len(targets))+b''.join(struct.pack('<QQ',t,total-t) for t in targets))
 result=subprocess.run([str(exe),'oggseek',str(path),str(q)],capture_output=True);assert result.returncode==0,result.stderr
 b=Reader(result.stdout);assert (b.num(4),b.num(4),b.num(4),b.num(4),b.num())==(1,97,channels,48000,total);seeks=[]
 for t in targets:
  position=b.num();got=[bytearray() for _ in planes];frames_total=0
  while True:
   frames=b.num(4)
   if not frames:break
   assert b.num(4)==0;frames_total+=frames
   for plane in got:plane.extend(b.take(frames*4))
  assert position==t and frames_total==total-t and all(canonical(a)==b[t*4:] for a,b in zip(got,planes)),(i,t)
  seeks.append({'target':t,'position':position,'frames':frames_total,'canonical_raw_suffix_identical':True})
 b.done();container.append({'stream':i,'sha256':sha(data),'frames':total,'channels':channels,'raw_canonical_pcm_identical':True,'seeks':seeks})
 pcm.append({'case':i,'request_sha256':sha(raw),'response_sha256':sha(out),'fresh_pcm_identical':True})
a.done()


summary={'scope':'Every floor-one callback batch and retained golden recaptured with fresh C processes; single-record probes cover header/block/alignment/presence/EOF and flat/zero/unit-curve categories. All preceding procedural request bytes and 34 older PCM helper responses remain unchanged. Both C helpers agree on the six new PCM goldens; C Ogg decodes and 84 C seeks agree exactly with raw synthesis.', 'reference_binary_sha256':sha(exe.read_bytes()),'previous_binary_sha256':sha(old.read_bytes()),'component_cases':case_count,'component_values':value_count,'component_matrix':matrix,'component_fresh_records':singles,'component_goldens':component_goldens,'unchanged_procedural_inputs':unchanged,'old_pcm_regressions':compatibility,'pcm_goldens':pcm,'container_reference_checks':container,'goldens':{p.name:{'bytes':p.stat().st_size,'sha256':sha(p.read_bytes())} for p in [fixture/'floor1-components-v1.libvorbis',fixture/'floor1-pcm-v1.libvorbis-pcm']}}
(run/'independent-reference.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf8',newline='\n')
print('Verified',case_count,'floor-one callback cases;',len(singles),'fresh records;',len(unchanged),'unchanged requests;',len(compatibility),'old PCM responses; six C Ogg streams and',sum(len(x['seeks']) for x in container),'C seeks.')
