from pathlib import Path
import hashlib,json,shutil,struct,subprocess
repo=Path.cwd();root=repo/'target/deadlib-audio-reference';source=root/'eos-clamping-seeds-verified-20261027';run=root/'eos-clamping-selected-seeds-final-20261027';run.mkdir(exist_ok=False)
for name in ['vorbis_headers','vorbis_packets','vorbis_stream']:shutil.copytree(root/'fuzz-floor1-guards-20261027/corpus'/name,run/name)
sha=lambda b:hashlib.sha256(b).hexdigest();all=json.loads((source/'targeted.json').read_text(encoding='utf8'));selected=[]
corners={offset+i for offset in [0,315,630,945]for i in [0,45,90,91,94,115,180,181,182,183,184,185,186,187,188,189,190,198,225,270,314]}
for row in all['seeds']:
 if row['case'] in corners:
  shutil.copyfile(source/row['path'],run/row['path']);selected.append(row)
assert len(selected)==336
def crc(b):
 v=0
 for x in b:
  v^=x<<24
  for _ in range(8):v=((v<<1)^(0x04c11db7 if v&0x80000000 else 0))&0xffffffff
 return v
def page(p,i,gp):
 laces=bytes([255]*(len(p)//255)+[len(p)%255]);assert len(laces)<=255;b=bytearray(b'OggS'+bytes([0,2 if i==0 else 4 if i==5 else 0])+struct.pack('<QII',gp,193,i)+bytes(4)+bytes([len(laces)])+laces+p);b[22:26]=struct.pack('<I',crc(b));return b
b=(repo/'crates/deadlib-audio/decode/vorbis/tests/fixtures/stereo.ogg').read_bytes();at=0;pending=bytearray();packets=[]
while at<len(b):
 n=b[at+26];laces=b[at+27:at+27+n];body=at+27+n
 for size in laces:
  pending.extend(b[body:body+size]);body+=size
  if size<255:packets.append(bytes(pending));pending.clear()
 at=body
data=b''.join(page(p,i,0 if i<=3 else (1<<63)-11 if i==4 else (1<<63)-1)for i,p in enumerate(packets[:6]));p=run/'counter-wrap.ogg';p.write_bytes(data);exe=root/'vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe';r=subprocess.run([str(exe),'oggstream',str(p)],capture_output=True);assert r.returncode==0,r.stderr;p.with_suffix('.out').write_bytes(r.stdout);at=8;frames=0
while True:
 n=int.from_bytes(r.stdout[at:at+4],'little');at+=4
 if not n:break
 assert struct.unpack_from('<III',r.stdout,at)==(0,2,48000);at+=12+n*8;frames+=n
assert at==len(r.stdout)and frames==1600
for control in [128,255]:
 data=bytes([control])+p.read_bytes();name=f'vorbis_stream/counter-wrap-{control}.bin';(run/name).write_bytes(data);selected.append({'path':name,'sha256':sha(data),'bytes':len(data),'control':control,'case':'counter-wrap'})
(run/'selection.json').write_text(json.dumps({'scope':'Select 336 representative EOS controls from 5040 verified variants for mutation time, retaining the complete prior corpus. Add two C-verified signed-counter-wrap container seeds. All 1260 layouts remain covered by exact differential tests.','source_manifest_sha256':sha((source/'targeted.json').read_bytes()),'reference_binary_sha256':sha(exe.read_bytes()),'counter_wrap':{'ogg_sha256':sha(p.read_bytes()),'response_sha256':sha(r.stdout),'frames':frames},'seeds':selected},indent=2)+'\n',encoding='utf8',newline='\n');print('Selected',len(selected),'targeted seeds; C counter-wrap stream emits',frames,'frames.')
