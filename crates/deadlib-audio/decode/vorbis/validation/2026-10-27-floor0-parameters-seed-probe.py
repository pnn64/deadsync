from pathlib import Path
import hashlib,json,struct,subprocess,shutil
repo=Path.cwd();root=repo/'target/deadlib-audio-reference';run=root/'floor0-parameters-seeds-20261027';run.mkdir(exist_ok=False)
prior=root/'fuzz-codebook-extents-20261027/corpus'
for target in ['vorbis_headers','vorbis_packets','vorbis_stream','vorbis_encode']:shutil.copytree(prior/target,run/target,dirs_exist_ok=True)
exe=root/'vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe';sha=lambda b:hashlib.sha256(b).hexdigest()
def crc(data):
 v=0
 for b in data:
  v^=b<<24
  for _ in range(8):v=((v<<1)^(0x04c11db7 if v&0x80000000 else 0))&0xffffffff
 return v
def container(headers,packets,ends):
 pages=[]
 for i,data in enumerate(headers+packets):
  laces=bytes([255]*(len(data)//255)+[len(data)%255]);assert len(laces)<=255
  flags=(2 if i==0 else 0)|(4 if i==len(headers)+len(packets)-1 else 0)
  gp=0 if i<3 else ends[i-3]
  b=bytearray(b'OggS'+bytes([0,flags])+struct.pack('<QII',gp,97,i)+bytes(4)+bytes([len(laces)])+laces+data)
  b[22:26]=struct.pack('<I',crc(b));pages.append(b)
 return b''.join(pages)
records=[];probes=[]
for i in range(7):
 raw=(root/f'floor0-parameters-20261027/golden-pcm-inputs/floor0-golden-{i}.bin').read_bytes();at=0
 def u64():
  global at
  n=int.from_bytes(raw[at:at+8],'little');at+=8;return n
 def packet():
  global at
  n=u64();b=raw[at:at+n];at+=n;return b
 headers=[packet() for _ in range(3)];assert u64()==1;packets=[packet() for _ in range(u64())];assert at==len(raw)
 source=run/f'probe-{i}.bin';source.write_bytes(raw);out=subprocess.check_output([str(exe),'pcm-cases',str(source)]);source.with_suffix('.out').write_bytes(out)
 channels=int.from_bytes(out[:4],'little');assert int.from_bytes(out[4:12],'little')==1;atout=12;ends=[];total=0
 for p in packets:
  assert out[atout:atout+1]==b'P';block,frames=struct.unpack_from('<II',out,atout+1);atout+=9+frames*channels*4;total+=frames;ends.append(total)
 assert atout==len(out)
 data=container(headers,packets,ends)
 for target,controls,payload in [('vorbis_headers',[next(f+j for j in range(8) if (f+j)%3==2) for f in [0,8,32,40,64,72,104,232,248]],headers[0]+headers[2]),('vorbis_stream',[0,32,128,160,184,248],data)]:
  for control in controls:
   seed=bytes([control])+payload;assert len(seed)<=131073;name=f'floor0-{i}-control-{control}.bin';(run/target/name).write_bytes(seed)
   records.append({'path':target+'/'+name,'bytes':len(seed),'sha256':sha(seed),'case':i,'control':control})
 probes.append({'case':i,'frames':total,'channels':channels,'request_sha256':sha(raw),'response_sha256':sha(out),'ogg_sha256':sha(data)})
(run/'targeted.json').write_text(json.dumps({'scope':'Seven independent procedural floor-zero C PCM cases: isolated-infinity transform regression, signed/unsupported amplitude widths, Bark extremes, high-order LSP, coupling and empty-book policy. Header/stream controls exercise published and reference policies independently, with and without IEEE propagation. Container granules are derived from C frame counts.','reference_binary_sha256':sha(exe.read_bytes()),'probes':probes,'seeds':records},indent=2)+'\n',encoding='utf8',newline='\n')
print('Prepared',len(records),'targeted seeds.')
