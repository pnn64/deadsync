"""Independent wavl recipes and both physical/native and canonical sample truth."""
import struct
from pcm_fixtures import KINDS,make,chunk


def chunks(payload,container,data_bytes=0):
    e='>' if container=='RIFX' else '<';offset=12;table={}
    while offset<len(payload):
        tag=payload[offset:offset+4];size=struct.unpack_from(e+'I',payload,offset+4)[0]
        if tag==b'ds64':
            entries=struct.unpack_from('<I',payload,offset+8+24)[0]
            for i in range(entries):
                at=offset+8+28+12*i;table[payload[at:at+4]]=struct.unpack_from('<Q',payload,at+4)[0]
        if size==0xffffffff:size=data_bytes if tag==b'data' else table[tag]
        start=offset+8;end=start+size;assert end+(size&1)<=len(payload)
        yield tag,payload[start:end]
        offset=end+(size&1)
    assert offset==len(payload)


def pack(container,fmt,audio,frames,wavl=False,trailing_fact=False):
    e='>' if container=='RIFX' else '<';large=container in ('RF64','BW64')
    metadata=chunk(b'LIST',b'INFO'+chunk(b'INAM',b'wavl fixture\0',e),e)+chunk(b'JUNK',b'even00',e)
    fact=chunk(b'fact',struct.pack(e+'I',min(frames,0xffffffff)),e)
    body=chunk(b'fmt ',fmt,e)+metadata+(b'' if trailing_fact else fact)
    body+=chunk(b'LIST' if wavl else b'data',audio,e,sentinel=large)
    if trailing_fact:body+=fact
    body+=chunk(b'zzzz',b'tail',e)
    if large:
        table=b'LIST'+struct.pack('<Q',len(audio)) if wavl else b''
        ds=struct.pack('<QQQI',4+36+len(table)+len(body),0 if wavl else len(audio),frames,int(wavl))+table
        return container.encode()+b'\xff'*4+b'WAVE'+chunk(b'ds64',ds)+body
    return container.encode()+struct.pack(e+'I',4+len(body))+b'WAVE'+body


def fixture(container,kind,channels,source_frames=17,valid=None,trailing_fact=False,pad=0):
    info,base,canonical=make(container,kind,channels,source_frames,valid)
    original=dict(chunks(base,container,source_frames*channels*info['bits']//8));fmt=original[b'fmt '];audio=original[b'data']
    e='>' if container=='RIFX' else '<';align=channels*info['bits']//8;stride=channels*(8 if kind=='f64' else 4)
    if source_frames:
        split=max(1,source_frames//2)
        records=[(b'slnt',struct.pack(e+'I',3)),(b'data',audio[:split*align]),(b'JUNK',b'odd\0\xff'),(b'slnt',struct.pack(e+'I',5)),(b'data',b''),(b'slnt',struct.pack(e+'I',0)),(b'data',audio[split*align:]),(b'slnt',struct.pack(e+'I',2))]
        truth=b'\0'*(3*stride)+canonical[:split*stride]+canonical[(split-1)*stride:split*stride]*5+canonical[split*stride:]+canonical[-stride:]*2
        wire=bytes([128 if kind=='u8' else 0])*(3*align)+audio[:split*align]+audio[(split-1)*align:split*align]*5+audio[split*align:]+audio[-align:]*2
        frames=source_frames+10
    else:
        records=[(b'data',b''),(b'slnt',struct.pack(e+'I',0)),(b'JUNK',b'odd\0\xff')];truth=wire=b'';frames=0
    sequence=b'wavl'+b''.join(chunk(tag,payload,e,pad=pad) for tag,payload in records)
    canonical_sequence=b'wavl'+b''.join(chunk(tag,payload,e) for tag,payload in records)
    payload=pack(container,fmt,sequence,frames,True,trailing_fact);flat=pack(container,fmt,wire,frames)
    info.update(name=info['name']+'-wavl'+('-trailing-fact' if trailing_fact else '')+('-compat-pad' if pad else ''),frames=frames,source_frames=source_frames,trailing_fact=trailing_fact,pad_byte=pad,profile='compatibility' if pad else 'canonical',core_reference_gate=container=='RIFF' and channels<=2 and info['bits']==info['valid_bits'] and frames>0 and not pad)
    assert len(truth)==frames*stride and len(wire)==frames*align
    return info,payload,flat,truth,wire,canonical_sequence


def cases():
    for container in ('RIFF','RIFX','RF64','BW64'):
        for kind in KINDS:
            for channels in (1,2,6,33):yield fixture(container,kind,channels)
        for bits,valid in ((8,3),(16,12),(24,20),(24,23),(32,24),(32,1)):
            for channels in (1,2,6):yield fixture(container,{8:'u8',16:'i16',24:'i24',32:'i32'}[bits],channels,37,valid)
        for kind in KINDS:
            for channels in (1,2):yield fixture(container,kind,channels,trailing_fact=True)
            yield fixture(container,kind,1,source_frames=0)
        for kind in ('u8','f64'):yield fixture(container,kind,1,pad=0xa5)
