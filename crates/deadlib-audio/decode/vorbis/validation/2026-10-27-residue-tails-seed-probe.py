from pathlib import Path
import hashlib,json,struct,subprocess,shutil
repo=Path.cwd();root=repo/'target/deadlib-audio-reference';run=root/'residue-tails-seeds-verified-20261027';run.mkdir(exist_ok=False)
prior=root/'fuzz-codebook-floats-20261027/corpus'
for t in ['vorbis_headers','vorbis_packets','vorbis_stream','vorbis_encode']:shutil.copytree(prior/t,run/t,dirs_exist_ok=True)
exe=root/'vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe';sha=lambda b:hashlib.sha256(b).hexdigest()
class Reader:
 def __init__(self,b):self.b=b;self.at=0
 def take(self,n):v=self.b[self.at:self.at+n];self.at+=n;assert len(v)==n;return v
 def num(self,n=8):return int.from_bytes(self.take(n),'little')
 def packet(self):return self.take(self.num())
 def done(self):assert self.at==len(self.b)
def packed(items):return b''.join(struct.pack('<Q',len(b))+b for b in items)
def first(raw):
 r=Reader(raw);headers=[r.packet() for _ in range(3)];assert r.num()>0;packets=[r.packet() for _ in range(r.num())];return headers,packets
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
  flags=(2 if i==0 else 0)|(4 if i==len(headers)+len(packets)-1 else 0);gp=0 if i<3 else ends[i-3]
  b=bytearray(b'OggS'+bytes([0,flags])+struct.pack('<QII',gp,97,i)+bytes(4)+bytes([len(laces)])+laces+data);b[22:26]=struct.pack('<I',crc(b));pages.append(b)
 return b''.join(pages)
sources=sorted((root/'residue-tails-20261027/golden-pcm-inputs').glob('*.bin'))
sources += [root/'residue-tails-20261027/packet-inputs'/f'residue-tail-pcm-{i}-batch-0.bin' for i in range(144,180)]
records=[];probes=[]
for i,p in enumerate(sources):
 headers,packets=first(p.read_bytes());raw=packed(headers)+struct.pack('<QQ',1,len(packets))+packed(packets)
 q=run/f'probe-{i}.bin';q.write_bytes(raw);r=subprocess.run([str(exe),'pcm-cases',str(q)],capture_output=True);assert r.returncode==0,r.stderr;out=r.stdout;q.with_suffix('.out').write_bytes(out)
 r=Reader(out);channels=r.num(4);assert r.num()==1;ends=[];total=0
 for _ in packets:
  assert r.take(1)==b'P';block=r.num(4);frames=r.num(4);total+=frames;ends.append(total);r.take(channels*frames*4)
 r.done();assert total==368;data=container(headers,packets,ends)
 for target,controls,payload in [('vorbis_headers',[2,128,134,254],headers[0]+headers[2]),('vorbis_stream',[0,8,128,136,232,255],data)]:
  for control in controls:
   seed=bytes([control])+payload;assert len(seed)<=131073;name=f'residue-tail-{i}-control-{control}.bin';(run/target/name).write_bytes(seed);records.append({'path':target+'/'+name,'bytes':len(seed),'sha256':sha(seed),'case':i,'control':control})
 probes.append({'case':i,'source_request':p.relative_to(repo).as_posix(),'source_request_sha256':sha(p.read_bytes()),'frames':total,'channels':channels,'request_sha256':sha(raw),'response_sha256':sha(out),'ogg_sha256':sha(data)})
(run/'targeted.json').write_text(json.dumps({'scope':'Partial VQ partitions of sizes 1/2/3/5/7/11/12/13/17, inverted/empty ranges and the maximum partition field; all residue kinds and later active/empty submaps. C-derived granules and full PCM initialization/decision/count checks. Header controls preserve separate published/reference choices; stream bit seven selects reference residue behavior with codebook/floor0.', 'reference_binary_sha256':sha(exe.read_bytes()),'probes':probes,'seeds':records},indent=2)+'\n',encoding='utf8',newline='\n')
print('Prepared',len(records),'seeds from',len(probes),'C-checked sequences.')
