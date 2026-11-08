from pathlib import Path
import hashlib,json,struct,subprocess,shutil
repo=Path.cwd();root=repo/'target/deadlib-audio-reference';run=root/'codebook-floats-seeds-verified-20261027';run.mkdir(exist_ok=False)
prior=root/'fuzz-floor0-parameters-20261027/corpus'
for target in ['vorbis_headers','vorbis_packets','vorbis_stream','vorbis_encode']:shutil.copytree(prior/target,run/target,dirs_exist_ok=True)
exe=root/'vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe';sha=lambda b:hashlib.sha256(b).hexdigest()
class Reader:
 def __init__(self,b):self.b=b;self.at=0
 def take(self,n):b=self.b[self.at:self.at+n];self.at+=n;assert len(b)==n;return b
 def num(self,n=8):return int.from_bytes(self.take(n),'little')
 def packet(self):return self.take(self.num())
 def done(self):assert self.at==len(self.b)
def packed(items):return b''.join(struct.pack('<Q',len(b))+b for b in items)
def parse(raw):
 r=Reader(raw);headers=[r.packet() for _ in range(3)];assert r.num()==1;packets=[r.packet() for _ in range(r.num())];r.done();return headers,packets
def offsets(setup):
 assert setup[:7]==b'\x05vorbis';value=int.from_bytes(setup,'little');at=56
 def read(n):
  nonlocal at
  v=(value>>at)&((1<<n)-1);at+=n;return v
 fields=[]
 for _ in range(read(8)+1):
  assert read(24)==0x564342;dim=read(16);entries=read(24)
  if read(1):
   read(5);i=0
   while i<entries:i+=read((entries-i).bit_length())
  else:
   sparse=read(1)
   for _ in range(entries):
    if not sparse or read(1):read(5)
  lookup=read(4)
  if not lookup:continue
  assert lookup in [1,2];minimum=at;read(32);delta=at;read(32);width=read(4)+1;read(1)
  if lookup==2:count=entries*dim
  else:
   count=1
   while (count+1)**dim<=entries:count+=1
  at+=width*count;assert at<=len(setup)*8;fields.append((minimum,delta))
 return fields
def patch(setup,minimum,delta):
 value=int.from_bytes(setup,'little')
 for m,d in offsets(setup):
  for at,word in [(m,minimum),(d,delta)]:value=(value&~(0xffffffff<<at))|(word<<at)
 return value.to_bytes(len(setup),'little')
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
source=root/'codebook-floats-20261027/golden-pcm-inputs';variants=[]
for i in range(6):
 p=source/f'float-golden-{i}.bin';raw=p.read_bytes();headers,packets=parse(raw);variants.append((f'golden-{i}',headers,packets,sha(raw)))
base_headers,base_packets=parse((source/'float-golden-0.bin').read_bytes())
for exponent in [0,724,725,726,850,851,852,1023]:
 for sign in [0,0x80000000]:
  for role in ['minimum','delta']:
   for mantissa in [1,0x1fffff]:
    word=sign|(exponent<<21)|mantissa
    minimum,delta=(word,0) if role=='minimum' else (0x80000000|(787<<21)|1,word)
    headers=base_headers[:2]+[patch(base_headers[2],minimum,delta)]
    variants.append((f'exponent-{exponent}-sign-{sign}-role-{role}-mantissa-{mantissa}',headers,base_packets,None))
records=[];probes=[]
for i,(label,headers,packets,source_sha) in enumerate(variants):
 raw=packed(headers)+struct.pack('<QQ',1,len(packets))+packed(packets);p=run/f'probe-{i}.bin';p.write_bytes(raw)
 out=subprocess.check_output([str(exe),'pcm-cases',str(p)]);p.with_suffix('.out').write_bytes(out)
 r=Reader(out);channels=r.num(4);assert r.num()==1;ends=[];total=0
 for _ in packets:
  assert r.take(1)==b'P';block=r.num(4);frames=r.num(4);total+=frames;ends.append(total);r.take(channels*frames*4)
 r.done();assert total==368
 data=container(headers,packets,ends)
 for target,controls,payload in [('vorbis_headers',[2,5,233,236,248,254],headers[0]+headers[2]),('vorbis_stream',[0,32,128,168],data)]:
  for control in controls:
   seed=bytes([control])+payload;assert len(seed)<=131073;name=f'float-{i}-control-{control}.bin';(run/target/name).write_bytes(seed)
   records.append({'path':target+'/'+name,'bytes':len(seed),'sha256':sha(seed),'case':i,'control':control})
 probes.append({'case':i,'label':label,'frames':total,'channels':channels,'request_sha256':sha(raw),'response_sha256':sha(out),'ogg_sha256':sha(data),'source_request_sha256':source_sha})
# Preserve both sides of the private byte guard as deliberately truncated setup
# inputs. These are not counted as accepted complete Vorbis headers.
request=root/'codebook-floats-20261027/golden-inputs/float-golden-guards.bin';response=request.with_suffix('.out');a=Reader(request.read_bytes());b=Reader(response.read_bytes());assert a.num()==b.num()==83968
selected={False:[],True:[]};guards=[]
for i in range(83968):
 prefix=a.num();book=a.packet();accepted=bool(b.num(1));position=b.num();initialized=bool(b.num(1));assert accepted==initialized
 if prefix==0 and len(selected[accepted])<4:
  selected[accepted].append(i)
  for control in [2,5]:
   seed=bytes([control])+base_headers[0]+b'\x05vorbis\0'+book;name=f'float-guard-{i}-control-{control}.bin';(run/'vorbis_headers'/name).write_bytes(seed)
   records.append({'path':'vorbis_headers/'+name,'bytes':len(seed),'sha256':sha(seed),'guard_case':i,'control':control})
  guards.append({'case':i,'bare_book_accepted':accepted,'consumed_bits':position,'book_sha256':sha(book)})
a.done();b.done();assert all(len(v)==4 for v in selected.values());assert len(records)==716
(run/'targeted.json').write_text(json.dumps({'scope':'Six independent PCM golden setups plus 64 independently patched extreme-exponent/sign/field cases, C-verified full initialization/decoding/counts. Header controls isolate reference codebooks and full compatibility combinations; stream controls select published/reference codebook and floor behavior with separate IEEE/layout choices. Eight private byte-guard inputs retain both outcomes as deliberately incomplete setup fuzz inputs. Container granules derive from C frame counts.','reference_binary_sha256':sha(exe.read_bytes()),'probes':probes,'guard_source_request_sha256':sha(request.read_bytes()),'guard_source_response_sha256':sha(response.read_bytes()),'guard_cases':guards,'seeds':records},indent=2)+'\n',encoding='utf8',newline='\n')
print('Prepared',len(records),'targeted seeds and',len(probes),'C initialization/PCM/count probes.')
