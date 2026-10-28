from pathlib import Path
import hashlib,json,struct,shutil,subprocess
repo=Path.cwd();root=repo/'target/deadlib-audio-reference';run=root/'seek-reference-20261027';out=root/'synthesis-state-seeds-20261027';out.mkdir(exist_ok=False)
sha=lambda b:hashlib.sha256(b).hexdigest()
for target in ['vorbis_packets','vorbis_stream']:
 shutil.copytree(root/'fuzz-eos-clamping-final-20261027/corpus'/target,out/target)
(out/'vorbis_synthesis').mkdir()
class R:
 def __init__(self,b):self.b=b;self.at=0
 def take(self,n):b=self.b[self.at:self.at+n];self.at+=n;assert len(b)==n;return b
 def num(self):return int.from_bytes(self.take(8),'little')
 def blob(self):return self.take(self.num())
def headers(b):
 at=0;pending=bytearray();parts=[]
 while len(parts)<3:
  n=b[at+26];laces=b[at+27:at+27+n];at+=27+n
  for size in laces:
   pending.extend(b[at:at+size]);at+=size
   if size<255:parts.append(bytes(pending));pending.clear()
   if len(parts)==3:return parts
fixtures=[headers((repo/f'crates/deadlib-audio/decode/vorbis/tests/fixtures/{name}.ogg').read_bytes())for name in ['stereo','mono','surround','eight-channel']]
requests=sorted((run/'captured-final').glob('*/schedule.bin'))
requests += sorted((run/'component-inputs').glob('synthesis-fixture-*-batch-*.bin'))
rows=[];seeds=[]
for p in requests:
 r=R(p.read_bytes());h=[r.blob()for _ in range(3)];fixture=fixtures.index(h);wire_cases=[]
 for _ in range(r.num()):
  events=[]
  for _ in range(r.num()):
   kind=r.num()
   if kind in [0,7]:event=b'\0'
   elif kind in [1,2]:
    seq=r.num();gp=r.num();eos=r.num();payload=r.blob();assert len(payload)<=65535
    event=bytes([kind])+struct.pack('<HQQB',len(payload),seq,gp,int(eos!=0))+payload
   elif kind==3:event=b'\3'+struct.pack('<I',r.num())
   elif kind==5:event=b'\4'
   else:raise AssertionError(kind)
   events.append(event)
  wire_cases.append(events)
 assert r.at==len(r.b)
 # Every original C request is recaptured unchanged before seed conversion.
 rec=subprocess.run([str(root/'vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe'),'synthesis-ops',str(p)],capture_output=True);assert rec.returncode==0
 assert rec.stdout==p.with_suffix('.out').read_bytes(),p
 rows.append({'request':p.relative_to(repo).as_posix(),'request_sha256':sha(r.b),'response_sha256':sha(rec.stdout)})
 # Deterministic boundary cases plus randomized late cases, bounded for startup.
 indices=sorted({0,min(1,len(wire_cases)-1),len(wire_cases)-1})
 for case in indices:
  events=wire_cases[case]
  for start in range(0,len(events),31):
   for ieee in [False,True]:
    b=bytes([fixture |(64 if ieee else 0)])+b'\0'+b''.join(events[start:start+31]);assert len(b)<=131073
    digest=sha(b);(out/'vorbis_synthesis'/digest).write_bytes(b)
    seeds.append({'name':digest,'source':p.relative_to(repo).as_posix(),'case':case,'operation_start':start,'ieee':ieee,'bytes':len(b)})
manifest={'scope':'Unchanged C synthesis requests/responses checked before converting packet/track/read/restart operations into bounded fuzz schedules. Fresh/restart become restart and each segment starts with restart. Fuzz determinism compares paired contexts with identical retained-buffer histories; it is not a separate C PCM oracle.','reference_binary_sha256':sha((root/'vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe').read_bytes()),'requests':rows,'seeds':seeds,'unique_synthesis_seeds':len(list((out/'vorbis_synthesis').iterdir())),'imported_seeds':{target:len(list((out/target).iterdir()))for target in ['vorbis_packets','vorbis_stream']}}
(out/'checks.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf8',newline='\n');print(len(rows),'C requests;',manifest['unique_synthesis_seeds'],'unique synthesis seeds')
