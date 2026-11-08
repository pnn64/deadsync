from pathlib import Path
import struct,subprocess,json,hashlib
repo=Path.cwd();root=repo/'target/deadlib-audio-reference';run=root/'reference-seek-20261027';out=root/'buffered-sync-seeds-20261028';(out/'ogg_sync').mkdir(parents=True,exist_ok=False);(out/'ogg_container').mkdir()
sha=lambda b:hashlib.sha256(b).hexdigest()
class R:
 def __init__(self,b):self.b=b;self.at=0
 def take(self,n):b=self.b[self.at:self.at+n];self.at+=n;assert len(b)==n;return b
 def num(self):return int.from_bytes(self.take(8),'little')
 def blob(self):return self.take(self.num())
 def done(self):assert self.at==len(self.b)
rows=[];seeds=[]
def put(target,b,label):
 assert len(b)<=131073
 h=sha(b);(out/target/h).write_bytes(b);seeds.append({'target':target,'sha256':h,'bytes':len(b),'source':label})
def chunks(events):
 part=[];length=6
 for event in events:
  if len(part)==63 or length+len(event)>120000:
   yield part;part=[];length=6
  part.append(event);length+=len(event)
 if part:yield part
for p in sorted((run/'sync-inputs').glob('*.bin')):
 label=p.stem
 # Selected read sizes/profiles retain all header versions and lacing layouts.
 if label.startswith('sync-state-')and not any(x in label for x in ['-chunk-27-profile-0','-chunk-255-profile-1','-chunk-65536-profile-2']):continue
 if label.startswith('sync-reader-')and not any(label.endswith(x)for x in ['-read-1','-read-2048','-read-65536']):continue
 if label.startswith('sync-golden'):continue
 mode='sync-read-ops'if label.startswith('sync-reader-')else'sync-ops'
 q=p.read_bytes();r=R(q);result=subprocess.run([str(root/'ogg-oracle-build/Release/deadlib_ogg_oracle.exe'),mode,str(p)],capture_output=True);assert result.returncode==0 and result.stdout==p.with_suffix('.out').read_bytes(),label
 rows.append({'label':label,'mode':mode,'request':p.relative_to(repo).as_posix(),'request_sha256':sha(q),'response_sha256':sha(result.stdout)})
 if mode=='sync-ops':
  events=[];stream=bytearray()
  for _ in range(r.num()):
   kind=r.num()
   if kind==1:
    b=r.blob();assert len(b)<=65535;events.append(b'\1'+struct.pack('<H',len(b))+b);stream.extend(b)
   else:assert kind in[0,2,3];events.append(bytes([kind]))
  r.done()
  for i,part in enumerate(chunks(events)):
   put('ogg_sync',b'\4'+struct.pack('<I',0)+b'\0'+b''.join(part),f'{label}/segment-{i}')
  # Container recovery sees the same complete transport bytes and checksums.
  if len(stream)<=131072:
   for control in [0,255]:put('ogg_container',bytes([control])+stream,label)
 else:
  source=r.blob();size=r.num();count=r.num();events=[]
  for _ in range(count):
   kind=r.num()
   if kind==0:
    bound=r.num();events.append(b'\4'+struct.pack('<I',0xffffffff if bound==0xffffffffffffffff else min(bound,0xfffffffe)))
   elif kind==1:events.append(b'\5'+struct.pack('<I',r.num()))
   else:assert kind in[2,3]
  r.done();control={1:0,2048:4,65536:5}[size]
  # The original source/driver schedule is checked above. Fuzz source state is
  # fresh for each bounded segment; injected IO is covered by normal C tests.
  for i in range(0,len(events),63):put('ogg_sync',bytes([control])+struct.pack('<I',len(source))+source+b'\0'+b''.join(events[i:i+63]),f'{label}/segment-{i//63}')
manifest={'scope':'Fresh unchanged C request/response recaptures before conversion to bounded fuzz schedules. Full version/flag and lacing data retained. Fuzz segments restart state and omit injected source errors; normal C tests verify those operations. No claim of an identical C DSP schedule after seed conversion.','reference_binary_sha256':sha((root/'ogg-oracle-build/Release/deadlib_ogg_oracle.exe').read_bytes()),'requests':rows,'seeds':seeds,'unique_seeds':{target:len(list((out/target).iterdir()))for target in ['ogg_sync','ogg_container']}}
(out/'checks.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf8',newline='\n');print(len(rows),'C schedules;',manifest['unique_seeds'])
