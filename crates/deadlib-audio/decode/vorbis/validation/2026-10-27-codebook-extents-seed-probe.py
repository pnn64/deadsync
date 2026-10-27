from pathlib import Path
import hashlib,json,struct,subprocess,shutil
repo=Path.cwd();root=repo/'target/deadlib-audio-reference';run=root/'codebook-extents-seeds-20261027';run.mkdir(exist_ok=True)
prior=root/'fuzz-degenerate-books-20261027/corpus'
for target in ['vorbis_headers','vorbis_packets','vorbis_stream','vorbis_encode']:shutil.copytree(prior/target,run/target,dirs_exist_ok=True)
exe=root/'vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe';sha=lambda b:hashlib.sha256(b).hexdigest()
base=root/'degenerate-books-20261027/golden-inputs/book-golden-2.bin';raw=base.read_bytes();at=0
def u64():
 global at
 n=int.from_bytes(raw[at:at+8],'little');at+=8;return n
def packet():
 global at
 n=u64();b=raw[at:at+n];at+=n;return b
headers=[packet() for _ in range(3)];assert u64()==1;packets=[packet() for _ in range(u64())];assert at==len(raw)
setup=int.from_bytes(headers[2],'little');offset=588;old_length=143
assert (setup>>offset)&0xffffff==0x564342 and (setup>>(offset+24))&0xffff==0 and (setup>>(offset+40))&0xffffff==0
assert (setup>>(offset+64))&1==1 and (setup>>(offset+70))&15==1
assert (setup>>(offset+74))&0xffffffff==0

def packed_book(dim,entries,style):
 v=0x564342|(dim<<24)|(entries<<40)|(1<<65);pos=66
 if style=='first':v|=1<<pos;pos+=entries+5
 elif style=='last':v|=1<<(pos+entries-1);pos+=entries+5
 else:assert style=='empty';pos+=entries
 return v,pos+4

def crc(data):
 v=0
 for b in data:
  v^=b<<24
  for _ in range(8):v=((v<<1)^(0x04c11db7 if v&0x80000000 else 0))&0xffffffff
 return v

def ogg(headers,packets):
 pages=[];seq=0;gp=0
 for i,data in enumerate(headers+packets):
  if i>=3:gp += [0,80,128,80,80][i-3]
  laces=bytes([255]*(len(data)//255)+[len(data)%255]);at=0
  for start in range(0,len(laces),255):
   seg=laces[start:start+255];last=start+255>=len(laces);n=sum(seg)
   flags=(2 if i==0 else 0)|(1 if start else 0)|(4 if i==7 and last else 0)
   b=bytearray(b'OggS'+bytes([0,flags])+struct.pack('<QII',gp if last else 0xffffffffffffffff,83,seq)+bytes(4)+bytes([len(seg)])+seg+data[at:at+n]);at+=n;seq+=1
   b[22:26]=struct.pack('<I',crc(b));pages.append(b)
  assert at==len(data)
 assert gp==368
 return b''.join(pages)

shapes=[(65535,e,s) for e in [127,128,255,256,257] for s in ['first','last','empty']]+[(8,(1<<19)+1,'first'),(0,1,'first')]
records=[];probes=[]
for i,(dim,entries,style) in enumerate(shapes):
 book,length=packed_book(dim,entries,style)
 # Independent header splicing: verify the source book's fields and exact bit
 # extent above, without using native parsing or encoding to build these seeds.
 value=(setup&((1<<offset)-1))|(book<<offset)|((setup>>(offset+old_length))<<(offset+length))
 bits=len(headers[2])*8-old_length+length;modified=headers[:2]+[value.to_bytes((bits+7)//8,'little')]
 request=b''.join(struct.pack('<Q',len(h))+h for h in modified);p=run/'probe-input.bin';p.write_bytes(request)
 result=subprocess.run([str(exe),'pcm',str(p)],capture_output=True)
 accepted=result.returncode==0
 if accepted:assert result.stdout==struct.pack('<I',2)
 else:assert result.stderr.strip()==b'PCM header'
 probes.append({'case':i,'dimensions':dim,'entries':entries,'style':style,'reference_setup_accepted':accepted,'request_sha256':sha(request)})
 data=ogg(modified,packets)
 for target,controls,payload in [('vorbis_headers',[2,17,224,242],modified[0]+modified[2]),('vorbis_stream',[0,16,64,80,56,120],data)]:
  for control in controls:
   seed=bytes([control])+payload;assert len(seed)<=131073
   name=f'extent-{i}-control-{control}.bin';(run/target/name).write_bytes(seed)
   records.append({'path':target+'/'+name,'bytes':len(seed),'sha256':sha(seed),'case':i,'control':control})
(run/'targeted.json').write_text(json.dumps({'scope':'Seventeen independently spliced unused scalar-book setups, both sides of the reference extent guard, empty/nonempty and zero-dimensional books, and page-spanning setup. Header cap controls 2/17/224/242 and stream 0/16/64/80/56/120. Base packet granules from prior C golden (368 frames); over-cap setups are accepted only under the explicit native extent override. Other element caps remain active.','base_request_sha256':sha(raw),'reference_binary_sha256':sha(exe.read_bytes()),'probes':probes,'seeds':records},indent=2)+'\n',encoding='utf8',newline='\n')
print('Prepared',len(records),'targeted seeds;',sum(p['reference_setup_accepted'] for p in probes),'of',len(probes),'setups accepted by C initialization.')
