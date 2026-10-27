from pathlib import Path
import hashlib,json,math,struct,subprocess
repo=Path.cwd();root=repo/'target/deadlib-audio-reference';run=root/'bit-mutations-parity-20261027';exe=root/'vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
records=[]
for lookup in [1,2]:
    fields=[(0x564342,24),(3,16),(1,24),(0,1),(0,1),(0,5),(lookup,4),((788<<21)|1,32),((764<<21)|1,32),(0,4),(1,1)]
    fields += [(1,1)]*(1 if lookup==1 else 3)
    value=0;count=0;wire=bytearray()
    for v,n in fields:
        value|=v<<count;count+=n
        while count>=8:wire.append(value&255);value>>=8;count-=8
    if count:wire.append(value&255)
    setup=b'\x05vorbis'+b'\0'+bytes(wire);request=len(setup).to_bytes(8,'little')+setup;p=run/f'sequence-book-{lookup}.bin';p.write_bytes(request);q=subprocess.run([str(exe),'books',str(p)],capture_output=True);assert q.returncode==0,q.stderr
    assert struct.unpack_from('<III',q.stdout)==(1,3,1) and q.stdout[12:14]==bytes([lookup,1])
    assert struct.unpack_from('<fff',q.stdout,14)==(1,2,3)
    records.append({'lookup_type':lookup,'input_sha256':sha(p),'output_sha256':hashlib.sha256(q.stdout).hexdigest(),'reference_vector':[1,2,3]})
(run/'sequence-books-reference.json').write_text(json.dumps({'binary_sha256':sha(exe),'minimum':1,'delta':'2^-24','dimensions':3,'sequence':True,'all_multiplicands':1,'books':records},indent=2)+'\n',encoding='utf8',newline='\n')
source=root/'corpus/floor0/vorbis_floor_type_0.ogg';data=source.read_bytes();at=0;part=bytearray();packets=[]
while at<len(data):
    assert data[at:at+4]==b'OggS';count=data[at+26];laces=data[at+27:at+27+count];body=at+27+count
    for n in laces:
        part.extend(data[body:body+n]);body+=n
        if n<255:packets.append(bytes(part));part.clear()
    at=body
audio=packets[3:];mut=bytearray(audio[27]);mut[2]^=1;case=[*audio[25:27],bytes(mut),*audio[28:30]];request=b''.join(len(x).to_bytes(8,'little')+x for x in [*packets[:3],*case]);p=run/'nonfinite-case.bin';p.write_bytes(request);q=subprocess.run([str(exe),'pcm',str(p)],capture_output=True);assert q.returncode==0,q.stderr
wire=q.stdout;channels=struct.unpack_from('<I',wire)[0];at=4;records=[]
for i in range(len(case)):
    kind=chr(wire[at]);at+=1
    if kind=='S':records.append({'step':i,'kind':kind});continue
    block,frames=struct.unpack_from('<II',wire,at);at+=8;values=[v[0] for v in struct.iter_unpack('<f',wire[at:at+frames*channels*4])];at+=len(values)*4
    records.append({'step':i,'kind':kind,'block':block,'frames':frames,'nonfinite':sum(not math.isfinite(v) for v in values),'peak_finite':max((abs(v) for v in values if math.isfinite(v)),default=0)})
assert at==len(wire) and sum(r.get('nonfinite',0) for r in records)==1600
m={'case':'floor-zero/packet-27/bit-16','source_sha256':sha(source),'input_sha256':sha(p),'response_sha256':hashlib.sha256(wire).hexdigest(),'reference_binary_sha256':sha(exe),'records':records}
(run/'nonfinite-case-reference.json').write_text(json.dumps(m,indent=2)+'\n',encoding='utf8',newline='\n')
print('Both sequence-book types match [1,2,3]; legacy corrupted-case reference emits 1,600 nonfinite sample values.')
