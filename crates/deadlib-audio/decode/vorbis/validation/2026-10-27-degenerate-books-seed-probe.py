from pathlib import Path
import hashlib,json,subprocess,struct,shutil
repo=Path.cwd();root=repo/'target/deadlib-audio-reference'
run=root/'degenerate-books-seeds-20261027';run.mkdir(exist_ok=False)
prior=root/'fuzz-residue-geometry-20261027/corpus'
for target in ['vorbis_headers','vorbis_packets','vorbis_stream','vorbis_encode']:
    shutil.copytree(prior/target,run/target)
exe=root/'vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe'
sha=lambda b:hashlib.sha256(b).hexdigest()
def unpack(raw):
    at=0
    def u64():
        nonlocal at
        n=int.from_bytes(raw[at:at+8],'little');at+=8;return n
    def packet():
        nonlocal at
        n=u64();b=raw[at:at+n];at+=n;return b
    headers=[packet() for _ in range(3)];cases=[]
    for _ in range(u64()):cases.append([packet() for _ in range(u64())])
    assert at==len(raw)
    return headers,cases
def response(raw,cases):
    channels,count=struct.unpack_from('<IQ',raw);assert count==len(cases);at=12;out=[]
    for case in cases:
        records=[]
        for _ in case:
            kind=raw[at];at+=1
            if kind==ord('S'):records.append(0)
            else:
                assert kind==ord('P');n,frames=struct.unpack_from('<II',raw,at);at+=8+frames*channels*4;records.append(frames)
        out.append(records)
    assert at==len(raw)
    return out
def crc(data):
    v=0
    for b in data:
        v^=b<<24
        for _ in range(8):v=((v<<1)^(0x04c11db7 if v&0x80000000 else 0))&0xffffffff
    return v
def stream(headers,packets,counts):
    pages=[];seq=0;gp=0
    for index,data in enumerate(headers+packets):
        if index>=3:gp+=counts[index-3]
        laces=bytes([255]*(len(data)//255)+[len(data)%255]);at=0
        for offset in range(0,len(laces),255):
            segment=laces[offset:offset+255];length=sum(segment);last=offset+255>=len(laces)
            flags=(2 if index==0 else 0)|(1 if offset else 0)|(4 if index==len(headers+packets)-1 and last else 0)
            b=bytearray(b'OggS'+bytes([0,flags])+struct.pack('<QII',gp if last else 0xffffffffffffffff,71,seq)+bytes(4)+bytes([len(segment)])+segment+data[at:at+length]);at+=length;seq+=1
            b[22:26]=struct.pack('<I',crc(b));pages.append(b)
        assert at==len(data)
    return b''.join(pages),gp
records=[]
def seed(target,name,data,source,raw,reference,gp):
    assert len(data)<=131073,(name,len(data))
    (run/target/name).write_bytes(data)
    records.append({'path':target+'/'+name,'bytes':len(data),'sha256':sha(data),'source_request':source.relative_to(repo).as_posix(),'source_sha256':sha(raw),'reference_pcm_sha256':sha(reference),'final_granule':gp})
for p in sorted((root/'degenerate-books-20261027/golden-inputs').glob('*.bin')):
    raw=p.read_bytes();headers,cases=unpack(raw)
    reference=subprocess.check_output([str(exe),'pcm-cases',str(p)]);counts=response(reference,cases)
    for control in [2,65,98,224]:
        seed('vorbis_headers',f'synthetic-{p.stem}-control-{control}.bin',bytes([control])+headers[0]+headers[2],p,raw,reference,None)
    for i,(packets,count) in enumerate(zip(cases,counts)):
        ogg,gp=stream(headers,packets,count);assert gp==368
        for control in [0,16,48,56]:
            seed('vorbis_stream',f'synthetic-{p.stem}-case-{i}-control-{control}.bin',bytes([control])+ogg,p,raw,reference,gp)
# Downloaded floor0 data remain in ignored storage; only hashes/provenance are retained.
p=root/'degenerate-books-20261027/matrix-inputs/nonfinite-reference.bin'
raw=p.read_bytes();headers,cases=unpack(raw)
reference=subprocess.check_output([str(exe),'pcm-cases',str(p)]);counts=response(reference,cases)
for i,(packets,count) in enumerate(zip(cases,counts)):
    ogg,gp=stream(headers,packets,count)
    for control in [32,40]:
        seed('vorbis_stream',f'external-floor0-ieee-{i}-control-{control}.bin',bytes([control])+ogg,p,raw,reference,gp)
(run/'targeted.json').write_text(json.dumps({'scope':'Eleven procedural degenerate-codebook configurations, a later-floor EOF recovery case and eleven pinned external floor0 nonfinite cases. Header and stream controls exercise strict, empty-book, IEEE and combined residue-layout policies. Counts/granules come independently from libvorbis. No external media redistributed. Inherits previous corpus without modifying it.','reference_binary_sha256':sha(exe.read_bytes()),'seeds':records},indent=2)+'\n',encoding='utf8',newline='\n')
print('prepared',len(records),'targeted seeds')
