from pathlib import Path
import hashlib,json,subprocess,struct,shutil
repo=Path.cwd();root=repo/'target/deadlib-audio-reference'
run=root/'configuration-parity-seeds-20261027';run.mkdir(exist_ok=False)
prior=root/'fuzz-bit-mutations-final-20261027/corpus'
for target in ['vorbis_headers','vorbis_packets','vorbis_stream','vorbis_encode']:
    shutil.copytree(prior/target,run/target)
exe=root/'vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe'
def crc(data):
    v=0
    for b in data:
        v^=b<<24
        for _ in range(8):v=((v<<1)^(0x04c11db7 if v&0x80000000 else 0))&0xffffffff
    return v
def page(data,sequence,gp,flags):
    laces=bytes([255]*(len(data)//255)+[len(data)%255])
    assert len(laces)<=255
    b=bytearray(b'OggS'+bytes([0,flags])+struct.pack('<QII',gp,71,sequence)+bytes(4)+bytes([len(laces)])+laces+data)
    b[22:26]=struct.pack('<I',crc(b));return b
records=[]
for p in sorted((root/'setup-parity-20261027/golden-inputs').glob('*.bin')):
    raw=p.read_bytes();at=0
    def u64():
        global at
        v=int.from_bytes(raw[at:at+8],'little');at+=8;return v
    def packet():
        global at
        n=u64();v=raw[at:at+n];at+=n;return v
    headers=[packet() for _ in range(3)];assert len(headers[0])==30
    count=u64();assert count==1
    packets=[packet() for _ in range(u64())];assert at==len(raw)
    out=subprocess.check_output([str(exe),'pcm-cases',str(p)]);pos=12;assert int.from_bytes(out[:4],'little')==2
    gp=0;pages=[page(h,i,0,2 if i==0 else 0) for i,h in enumerate(headers)]
    for i,packet_data in enumerate(packets):
        kind=out[pos];pos+=1
        if kind==ord('P'):
            n,frames=struct.unpack_from('<II',out,pos);pos+=8+frames*2*4;gp+=frames
        else:assert kind==ord('S')
        pages.append(page(packet_data,3+i,gp,4 if i+1==len(packets) else 0))
    assert pos==len(out)
    for target,data in [('vorbis_headers',b'\x02'+headers[0]+headers[2]),('vorbis_stream',b'\x00'+b''.join(pages))]:
        name='synthetic-'+p.stem+'.bin';(run/target/name).write_bytes(data)
        records.append({'path':target+'/'+name,'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest(),'source_request':p.relative_to(repo).as_posix(),'source_sha256':hashlib.sha256(raw).hexdigest(),'reference_pcm_sha256':hashlib.sha256(out).hexdigest(),'final_granule':gp})
(run/'targeted.json').write_text(json.dumps({'scope':'Eight procedural setup regressions. Strict Ogg framing; packet granules from independent libvorbis PCM counts. Inherits prior corpus without modifying it.','reference_binary_sha256':hashlib.sha256(exe.read_bytes()).hexdigest(),'seeds':records},indent=2)+'\n',encoding='utf8',newline='\n')
print('prepared',len(records),'targeted seeds')
