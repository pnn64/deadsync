"""Independent extensible IMA headers, word packing and scalar sample truth."""
import random
import struct
from ima3_fixtures import STEP,decode_block as decode3,block as block3,pcm_source


def chunk(tag,payload,e='<',sentinel=False):
    return tag+struct.pack(e+'I',0xffffffff if sentinel else len(payload))+payload+b'\0'*(len(payload)&1)


def descriptor(bits,channels,block_bytes,mask,e='<',extensible=True,rate=8000):
    frames=1+(block_bytes//channels-4)*8//bits
    prefix=struct.pack(e+'HHIIHH',0xfffe if extensible else 17,channels,rate,rate*block_bytes//frames,block_bytes,bits)
    if not extensible:return prefix+struct.pack(e+'HH',2,frames)
    return prefix+struct.pack(e+'HHIIHH',22,frames,mask,17,0,16)+b'\x80\0\0\xaa\0\x38\x9b\x71'


def wave(container,fmt,audio,frames,trailing=False):
    e='>' if container=='RIFX' else '<';large=container in ('RF64','BW64')
    fact=chunk(b'fact',struct.pack(e+'I',frames),e);data=chunk(b'data',audio,e,large)
    body=chunk(b'fmt ',fmt,e)+chunk(b'JUNK',b'odd',e)+(data+fact if trailing else fact+data)+chunk(b'zzzz',b'end',e)
    if large:
        ds=chunk(b'ds64',struct.pack('<QQQI',40+len(body),len(audio),frames if container=='RF64' else 0,0))
        return container.encode()+b'\xff'*4+b'WAVE'+ds+body
    return container.encode()+struct.pack(e+'I',4+len(body))+b'WAVE'+body


def block(bits,headers,codes,e='<'):
    if bits==3:return block3(headers,codes,e)
    size=len(codes[0])//2;assert all(len(c)==size*2 and len(c)%8==0 for c in codes)
    packed=[sum(code<<(4*i) for i,code in enumerate(c)).to_bytes(size,'little') for c in codes]
    return b''.join(struct.pack(e+'hBB',value,index,0) for value,index in headers)+b''.join(p[c:c+4] for c in range(0,size,4) for p in packed)


def decode_block(bits,payload,channels,e='<',combined=False):
    if bits==3:return decode3(payload,channels,e,combined)
    assert len(payload)>=4*channels and (len(payload)-4*channels)%(4*channels)==0
    frames=1+(len(payload)-4*channels)*2//channels;planes=[]
    for c in range(channels):
        value,index,reserved=struct.unpack_from(e+'hBB',payload,c*4);assert index<=88 and reserved==0
        stream=b''.join(payload[offset+c*4:offset+c*4+4] for offset in range(channels*4,len(payload),channels*4));packed=int.from_bytes(stream,'little');plane=[value]
        for i in range(frames-1):
            code=(packed>>(4*i))&15;step=STEP[index]
            delta=((2*(code%8)+1)*step)//8 if combined else step//8+(step if code&4 else 0)+(step//2 if code&2 else 0)+(step//4 if code&1 else 0)
            value=max(-32768,min(32767,value+(-delta if code&8 else delta)))
            index=max(0,min(88,index+(-1,-1,-1,-1,2,4,6,8)[code%8]));plane.append(value)
        planes.append(plane)
    return [planes[c][f] for f in range(frames) for c in range(channels)]


def truth(bits,audio,channels,block_bytes,frames,e='<',combined=False):
    samples=[]
    for offset in range(0,len(audio),block_bytes):samples.extend(decode_block(bits,audio[offset:offset+block_bytes],channels,e,combined))
    assert len(samples)>=frames*channels;samples=samples[:frames*channels]
    return struct.pack('<'+'h'*len(samples),*samples)


def cases():
    for container in ('RIFF','RIFX','RF64','BW64'):
        e='>' if container=='RIFX' else '<'
        for bits in (3,4):
            for channels in (1,2,6,33):
                for which in range(5):
                    groups=1 if which<2 else 3;block_bytes=channels*(4+groups*(12 if bits==3 else 4));block_frames=1+groups*(32 if bits==3 else 8)
                    frames=(0,1,block_frames-1,block_frames+1,block_frames*2+1)[which]
                    mask=(0,(1<<channels)-1 if channels<32 else 0,0x80000000)[which%3]
                    rng=random.Random(0x4558494d+bits*7+channels*41+which);blocks=[]
                    for n in range((frames+block_frames-1)//block_frames):
                        headers=[((n*971+c*1777+12345)%65536-32768,(n*37+c*19)%89) for c in range(channels)]
                        codes=[[rng.randrange(1<<bits) for _ in range(block_frames-1)] for _ in range(channels)]
                        encoded=block(bits,headers,codes,e)
                        blocks.append(encoded[:4*channels] if which>=3 and n==frames//block_frames else encoded)
                    audio=b''.join(blocks);fmt=descriptor(bits,channels,block_bytes,mask,e)
                    for trailing in (False,True):
                        info=dict(name=f'{container}-ima-ext-{bits}-{channels}-case{which}-tail{int(trailing)}',container=container,bits=bits,channels=channels,block_bytes=block_bytes,block_frames=block_frames,frames=frames,mask=mask,trailing=trailing,encoded=False)
                        yield info,wave(container,fmt,audio,frames,trailing),truth(bits,audio,channels,block_bytes,frames,e),truth(bits,audio,channels,block_bytes,frames,e,True),wave(container,descriptor(bits,channels,block_bytes,0,e,False),audio,frames,trailing)


def encoded_truth(payload,bits,channels,block_bytes,frames):
    assert payload[:4]==b'RIFF' and struct.unpack_from('<I',payload,4)[0]+8==len(payload)
    chunks={};offset=12
    while offset<len(payload):
        tag=payload[offset:offset+4];n=struct.unpack_from('<I',payload,offset+4)[0];end=offset+8+n
        assert end<=len(payload) and tag not in chunks;chunks[tag]=payload[offset+8:end];offset=end+(n&1)
    assert offset==len(payload) and chunks[b'fmt ']==descriptor(bits,channels,block_bytes,0)
    assert struct.unpack('<I',chunks[b'fact'])[0]==frames
    block_frames=1+(block_bytes//channels-4)*8//bits
    assert len(chunks[b'data'])==((frames+block_frames-1)//block_frames)*block_bytes
    audio=chunks[b'data'];return truth(bits,audio,channels,block_bytes,frames),truth(bits,audio,channels,block_bytes,frames,combined=True),audio
