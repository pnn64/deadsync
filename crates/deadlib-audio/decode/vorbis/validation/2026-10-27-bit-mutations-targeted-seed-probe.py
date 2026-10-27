from pathlib import Path
import hashlib,json
repo=Path.cwd();root=repo/'target/deadlib-audio-reference';run=root/'bit-mutations-fuzz-seeds-20261027';run.mkdir(exist_ok=False)
for name in ['vorbis_headers','vorbis_packets','vorbis_stream']:(run/name).mkdir()
source=root/'corpus/floor0/vorbis_floor_type_0.ogg';data=source.read_bytes();at=0;packets=[];spans=[];parts=[];partial=bytearray();pages=[]
while at<len(data):
    start=at;assert data[at:at+4]==b'OggS';count=data[at+26];laces=data[at+27:at+27+count];body=at+27+count
    for size in laces:
        partial.extend(data[body:body+size]);parts.extend(range(body,body+size));body+=size
        if size<255:packets.append(bytes(partial));spans.append(parts);parts=[];partial.clear()
    pages.append((start,body));at=body
assert not partial
def crc(page):
    value=0
    for byte in page:
        value^=byte<<24
        for _ in range(8):value=((value<<1)^(0x04c11db7 if value&0x80000000 else 0))&0xffffffff
    return value
records=[]
for name,number,bit in [('rounding',25,1),('selector',25,13),('sequence',26,32),('nonfinite',27,16),('discard',27,0)]:
    packet=bytearray(packets[3+number]);packet[bit//8]^=1<<(bit&7)
    case=[*packets[3+number-2:3+number],bytes(packet),*packets[3+number+1:3+number+3]]
    raw=b'\x04'+b''.join(len(p).to_bytes(2,'little')+p for p in case)
    p=run/'vorbis_packets'/('floor0-'+name+'.bin');p.write_bytes(raw);assert len(raw)<=131073
    absolute=spans[3+number][bit//8];page_start,page_end=next(x for x in pages if x[0]<=absolute<x[1]);last=spans[3+number+2][-1];end=next(e for s,e in pages if s<=last<e);assert end<=131072
    container=bytearray(data[:end]);container[absolute]^=1<<(bit&7);container[page_start+22:page_start+26]=b'\0'*4;container[page_start+22:page_start+26]=crc(container[page_start:page_end]).to_bytes(4,'little');q=run/'vorbis_stream'/('floor0-'+name+'.bin');q.write_bytes(b'\x01'+container)
    records.append({'name':name,'audio_packet':number,'bit':bit,'raw':{'path':p.relative_to(repo).as_posix(),'sha256':hashlib.sha256(raw).hexdigest(),'bytes':len(raw)},'stream':{'path':q.relative_to(repo).as_posix(),'sha256':hashlib.sha256(q.read_bytes()).hexdigest(),'bytes':q.stat().st_size},'stream_scope':'Original complete-page prefix through recovery packets; repaired changed-page CRC, explicit zero-span policy, no invented EOS.'})
m={'source':source.relative_to(repo).as_posix(),'source_sha256':hashlib.sha256(data).hexdigest(),'raw_fixture_control':4,'requires_floor_zero_environment':True,'seeds':records}
(run/'manifest.json').write_text(json.dumps(m,indent=2)+'\n',encoding='utf8',newline='\n');print('Prepared ten targeted raw/stream seeds from independently reproduced legacy mutation cases.')
