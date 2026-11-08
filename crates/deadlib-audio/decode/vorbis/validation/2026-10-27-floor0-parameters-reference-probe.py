from pathlib import Path
import hashlib,json,struct,subprocess,math
repo=Path.cwd();root=repo/'target/deadlib-audio-reference';run=root/'floor0-parameters-20261027';gates=root/'floor0-parameters-validation-verified-20261027'
exe=root/'vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe';old=run/'oracle-before.exe';sha=lambda b:hashlib.sha256(b).hexdigest()
class Reader:
 def __init__(self,b):self.b=b;self.at=0
 def take(self,n):b=self.b[self.at:self.at+n];self.at+=n;assert len(b)==n;return b
 def num(self,n=8):return int.from_bytes(self.take(n),'little')
 def packet(self):return self.take(self.num())
 def done(self):assert self.at==len(self.b)
def oracle(mode,raw,binary=exe):
 p=run/'independent-input.bin';p.write_bytes(raw);r=subprocess.run([str(binary),mode,str(p)],capture_output=True);assert r.returncode==0,(mode,r.stderr);return r.stdout
def packed(items):return b''.join(struct.pack('<Q',len(b))+b for b in items)
def components(raw):
 r=Reader(raw);rows=[]
 for _ in range(r.num()):
  start=r.at;present=r.num(1);pos=r.num();eof=r.num(1);n=r.num(4);amp=r.num(4);maps=r.take(n*4);curve=r.take(n*4)
  rows.append({'raw':raw[start:r.at],'present':present,'bits':pos,'eof':eof,'bins':n,'amp':amp,'maps':maps,'curve':curve})
 r.done();return rows
# Reconstruct the entire retained component golden from fresh callback commands.
golden=repo/'crates/deadlib-audio/decode/vorbis/tests/fixtures';r=Reader((golden/'floor0-components-v1.libvorbis').read_bytes());assert r.take(8)==b'DVF0CMP1';count=r.num(4);capture=b'DVF0CMP1'+struct.pack('<I',count);fresh=0;component_requests=[]
for i in range(count):
 request=r.packet();expected=r.packet();out=oracle('floor0cases',request);assert out==expected;capture+=packed([request,out]);a=Reader(request);headers=[a.packet() for _ in range(3)];cases=[]
 for _ in range(a.num()):
  start=a.at;a.take(16);a.packet();cases.append(request[start:a.at])
 a.done();rows=components(out)
 for j in sorted({0,len(cases)//2,len(cases)-1}):
  single=packed(headers)+struct.pack('<Q',1)+cases[j];assert oracle('floor0cases',single)==struct.pack('<Q',1)+rows[j]['raw'];fresh+=1
 component_requests.append({'case':i,'cases':len(rows),'request_sha256':sha(request),'response_sha256':sha(out)})
r.done();assert capture==(golden/'floor0-components-v1.libvorbis').read_bytes()
# Recheck the complete 344-setup matrix independently of the Rust comparator.
matrices=[]
for p in sorted((gates/'floor0-inputs').glob('components-*.bin'),key=lambda p:int(p.stem.split('-')[-1])):
 raw=p.read_bytes();out=oracle('floor0cases',raw);assert out==p.with_suffix('.out').read_bytes();rows=components(out)
 matrices.append({'case':int(p.stem.split('-')[-1]),'cases':len(rows),'request_sha256':sha(raw),'response_sha256':sha(out)})
assert len(matrices)==344 and sum(x['cases'] for x in matrices)==39946
# Rebuild every integer map from fresh actual C callbacks, preserving duplicates.
maps={};bark_requests=[]
for p in sorted((run/'golden-inputs').glob('bark-*.bin')):
 raw=p.read_bytes();out=oracle('floor0cases',raw);assert out==p.with_suffix('.out').read_bytes();_,rate,size,long=p.stem.split('-');rate=int(rate);size=int(size)
 for row in components(out):
  n=row['bins'];v=struct.unpack('<'+'I'*n,row['maps']);assert max(v)<size and not row['present'] and not row['eof'] and row['bits']==6 and not any(row['curve'])
  key=(rate,size,n)
  if key in maps:assert maps[key]==v
  maps[key]=v
 bark_requests.append({'request_sha256':sha(raw),'response_sha256':sha(out)})
capture=b'DVF0MAP1'+struct.pack('<I',len(maps));differences=0;witness=None
for (rate,size,n),values in sorted(maps.items()):
 capture+=struct.pack('<HHH',rate,size,n)+struct.pack('<'+'H'*n,*values)
 bark=lambda v:13.1*math.atan(.00074*v)+2.24*math.atan(.0000000185*v*v)+.0001*v
 scale=size/bark(rate*.5)
 for i,expected in enumerate(values):
  published=min(size-1,math.floor(bark(rate*.5*i/n)*scale));differences+=published!=expected
  if published!=expected and witness is None:witness={'rate':rate,'grid':size,'bins':n,'bin':i,'published':published,'reference':expected}
assert capture==(golden/'floor0-bark-v1.libvorbis').read_bytes() and len(maps)==1088 and sum(len(v) for v in maps.values())==1436160 and differences==312
# Preserve the preceding helper's existing PCM protocol responses.
compatibility=[]
for directory,prefix in [('degenerate-books-20261027','book-golden-'),('residue-geometry-20261027','geometry-golden-')]:
 for p in sorted((root/directory/'golden-inputs').glob(prefix+'*.bin')):
  raw=p.read_bytes();out=oracle('pcm-cases',raw);assert out==oracle('pcm-cases',raw,old)
  compatibility.append({'request':p.relative_to(repo).as_posix(),'request_sha256':sha(raw),'response_sha256':sha(out)})
def canonical(plane):
 return b''.join(struct.pack('<I',0x7fc00000 if w&0x7fffffff>0x7f800000 else w) for (w,) in struct.iter_unpack('<I',plane))
pcm=[];container=[];capture=b'DVF0PCM1'+struct.pack('<I',7)
for i in range(7):
 p=run/'golden-pcm-inputs'/f'floor0-golden-{i}.bin';raw=p.read_bytes();out=oracle('pcm-cases',raw);capture+=packed([out]);a=Reader(raw);headers=[a.packet() for _ in range(3)];assert a.num()==1;packets=[a.packet() for _ in range(a.num())];a.done()
 assert oracle('pcm',packed(headers+packets))==out[:4]+out[12:]
 a=Reader(out);channels=a.num(4);assert a.num()==1;planes=[bytearray() for _ in range(channels)];ends=[0]
 for _ in packets:
  assert a.take(1)==b'P';n=a.num(4);frames=a.num(4);ends.append(ends[-1]+frames)
  for plane in planes:plane.extend(a.take(frames*4))
 a.done();planes=list(map(canonical,planes));total=ends[-1]
 data=(run/'focused-inputs'/f'floor0-stream-{i}.ogg').read_bytes();path=run/'independent-stream.ogg';path.write_bytes(data)
 result=subprocess.run([str(exe),'oggpcm',str(path)],capture_output=True);assert result.returncode==0,result.stderr
 a=Reader(result.stdout);assert (a.num(4),a.num(4),a.num(4),a.num(4),a.num())==(1,97,channels,48000,total)
 got=[bytearray() for _ in planes];frames_total=0
 while a.at<len(a.b):
  assert a.num(4)==0;frames=a.num(4);frames_total+=frames
  for plane in got:plane.extend(a.take(frames*4))
 assert frames_total==total and list(map(canonical,got))==planes
 targets=[0,1,total-1,total,0]+[v for e in ends[2:-1] for v in [e-1,e,e+1]];q=run/'independent-seeks.bin';q.write_bytes(struct.pack('<Q',len(targets))+b''.join(struct.pack('<QQ',t,total-t) for t in targets))
 result=subprocess.run([str(exe),'oggseek',str(path),str(q)],capture_output=True);assert result.returncode==0,result.stderr
 a=Reader(result.stdout);assert (a.num(4),a.num(4),a.num(4),a.num(4),a.num())==(1,97,channels,48000,total);seeks=[]
 for t in targets:
  position=a.num();got=[bytearray() for _ in planes];frames_total=0
  while True:
   frames=a.num(4)
   if not frames:break
   assert a.num(4)==0;frames_total+=frames
   for plane in got:plane.extend(a.take(frames*4))
  assert position==t and frames_total==total-t and all(canonical(a)==b[t*4:] for a,b in zip(got,planes)),(i,t)
  seeks.append({'target':t,'position':position,'frames':frames_total,'canonical_raw_suffix_identical':True})
 a.done();container.append({'stream':i,'sha256':sha(data),'frames':total,'channels':channels,'raw_canonical_pcm_identical':True,'seeks':seeks})
 pcm.append({'case':i,'request_sha256':sha(raw),'response_sha256':sha(out),'fresh_pcm_identical':True})
assert capture==(golden/'floor0-parameters-v1.libvorbis-pcm').read_bytes()
summary={'scope':'Fresh unmodified backend callbacks recapture all three goldens; selected records independently use fresh single-case processes. Full component matrices recaptured. C raw/ogg PCM and 98 repeated seeks agree at every value after canonicalizing NaN payloads. Earlier C PCM modes remain unchanged.','reference_binary_sha256':sha(exe.read_bytes()),'previous_binary_sha256':sha(old.read_bytes()),'component_goldens':component_requests,'fresh_single_case_commands':fresh,'component_matrix':matrices,'bark':{'requests':len(bark_requests),'maps':len(maps),'indices':sum(map(len,maps.values())),'published_reference_integer_differences':differences,'first_witness':witness,'requests_sha256':sha(json.dumps(bark_requests,sort_keys=True).encode())},'pcm_goldens':pcm,'old_pcm_regressions':compatibility,'container_reference_checks':container,'goldens':{p.name:{'bytes':p.stat().st_size,'sha256':sha(p.read_bytes())} for p in [golden/'floor0-components-v1.libvorbis',golden/'floor0-bark-v1.libvorbis',golden/'floor0-parameters-v1.libvorbis-pcm']}}
(run/'independent-reference.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf8',newline='\n')
print('Verified 69 component goldens,',fresh,'fresh records, 344 matrices, 1088 maps, 7 PCM goldens,',len(compatibility),'older responses, 7 Ogg streams and',sum(len(x['seeks']) for x in container),'C seeks.')
