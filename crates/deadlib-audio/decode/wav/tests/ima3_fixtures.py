"""Independent RIFFNEW 3-bit IMA packing, scalar arithmetic and fixture truth."""
import random
import struct

STEP=(7,8,9,10,11,12,13,14,16,17,19,21,23,25,28,31,34,37,41,45,50,55,60,66,
      73,80,88,97,107,118,130,143,157,173,190,209,230,253,279,307,337,371,408,449,
      494,544,598,658,724,796,876,963,1060,1166,1282,1411,1552,1707,1878,2066,2272,
      2499,2749,3024,3327,3660,4026,4428,4871,5358,5894,6484,7132,7845,8630,9493,
      10442,11487,12635,13899,15289,16818,18500,20350,22385,24623,27086,29794,32767)


def chunk(tag,payload,endian='<',sentinel=False):
    return tag+struct.pack(endian+'I',0xffffffff if sentinel else len(payload))+payload+b'\0'*(len(payload)&1)


def block(headers,codes,endian='<'):
    channels=len(headers);size=len(codes[0])*3//8
    assert all(len(c)==len(codes[0]) and len(c)%32==0 for c in codes)
    result=b''.join(struct.pack(endian+'hBB',sample,index,0) for sample,index in headers)
    # Form logical channel streams as large integers, then interleave 32-bit words.
    packed=[sum(code<<(3*i) for i,code in enumerate(c)).to_bytes(size,'little') for c in codes]
    return result+b''.join(packed[c][offset:offset+4] for offset in range(0,size,4) for c in range(channels))


def decode_block(payload,channels,endian='<',combined=False):
    assert len(payload)>=channels*4 and (len(payload)-channels*4)%(channels*12)==0
    frames=1+(len(payload)-channels*4)*8//(3*channels);planes=[]
    for c in range(channels):
        value,index,_=struct.unpack_from(endian+'hBB',payload,c*4)
        assert 0<=index<=88
        words=b''.join(payload[offset+c*4:offset+c*4+4] for offset in range(channels*4,len(payload),channels*4))
        packed=int.from_bytes(words,'little');plane=[value]
        for i in range(frames-1):
            code=(packed>>(i*3))&7;step=STEP[index]
            delta=((2*(code%4)+1)*step)//4 if combined else step//4+(step if code&2 else 0)+(step//2 if code&1 else 0)
            value=max(-32768,min(32767,value+(-delta if code&4 else delta)))
            index=max(0,min(88,index+(-1,-1,1,2)[code%4]));plane.append(value)
        planes.append(plane)
    return [planes[c][frame] for frame in range(frames) for c in range(channels)]


def decode_data(payload,channels,block_bytes,frames,endian='<',combined=False):
    samples=[]
    for offset in range(0,len(payload),block_bytes):
        samples.extend(decode_block(payload[offset:offset+block_bytes],channels,endian,combined))
    assert len(samples)>=frames*channels
    samples=samples[:frames*channels]
    return struct.pack('<'+'h'*len(samples),*samples)


def wave(container,channels,groups,blocks,frames):
    endian='>' if container=='RIFX' else '<';large=container in ('RF64','BW64')
    block_bytes=channels*(4+12*groups);block_frames=1+32*groups;audio=b''.join(blocks)
    fmt=struct.pack(endian+'HHIIHHHH',17,channels,8000,8000*block_bytes//block_frames,block_bytes,3,2,block_frames)
    body=chunk(b'fmt ',fmt,endian)+chunk(b'JUNK',b'odd\0\xff',endian)+chunk(b'fact',struct.pack(endian+'I',frames),endian)+chunk(b'data',audio,endian,large)+chunk(b'zzzz',b'end',endian)
    if large:
        ds=chunk(b'ds64',struct.pack('<QQQI',4+36+len(body),len(audio),frames,0))
        payload=container.encode()+b'\xff'*4+b'WAVE'+ds+body
    else:payload=container.encode()+struct.pack(endian+'I',4+len(body))+b'WAVE'+body
    return payload,audio,block_bytes


def cases():
    for container in ('RIFF','RIFX','RF64','BW64'):
        endian='>' if container=='RIFX' else '<'
        for channels in (1,2,6,33):
            for groups in (0,1,3):
                rng=random.Random(0x33494d41+channels*37+groups);blocks=[]
                for n in range(3):
                    headers=[(((-32768,0,32767)[n]+c*1217+32768)%65536-32768,(n*39+c*7)%89) for c in range(channels)]
                    codes=[[rng.randrange(8) for _ in range(groups*32)] for c in range(channels)]
                    blocks.append(block(headers,codes,endian))
                frames=3*(1+32*groups)
                if groups==3:
                    blocks.append(block([(-1234+c*61,c%89) for c in range(channels)],[[i%8 for i in range(32)] for c in range(channels)],endian));frames+=33
                frames-=7 if groups else 1
                payload,audio,block_bytes=wave(container,channels,groups,blocks,frames)
                info=dict(name=f'{container.lower()}-ima3-{channels}-g{groups}',container=container,channels=channels,groups=groups,frames=frames,block_bytes=block_bytes,encoded=False)
                yield info,payload,decode_data(audio,channels,block_bytes,frames,endian),decode_data(audio,channels,block_bytes,frames,endian,True)
    for channels in (1,2):
        blocks=[]
        for initial in (-32768,0,32767):
            for index in range(89):
                for code in range(8):
                    blocks.append(block([(initial,index)]*channels,[[code]*32 for _ in range(channels)]))
        frames=33*len(blocks);payload,audio,block_bytes=wave('RIFF',channels,1,blocks,frames)
        info=dict(name=f'riff-ima3-exhaustive-{channels}',container='RIFF',channels=channels,groups=1,frames=frames,block_bytes=block_bytes,encoded=False)
        yield info,payload,decode_data(audio,channels,block_bytes,frames),decode_data(audio,channels,block_bytes,frames,combined=True)


def pcm_source(channels,frames,signal):
    rng=random.Random(0x33454e43+channels*17+frames)
    samples=[(-12345 if signal=='constant' else (i*1871+c*947)%65536-32768 if signal=='ramp' else rng.randrange(-32768,32768)) for i in range(frames) for c in range(channels)]
    audio=struct.pack('<'+'h'*len(samples),*samples)
    body=b'WAVE'+chunk(b'fmt ',struct.pack('<HHIIHH',1,channels,8000,16000*channels,2*channels,16))+chunk(b'data',audio)
    return b'RIFF'+struct.pack('<I',len(body))+body,samples


def encoded_truth(payload,channels,frames,block_bytes,combined=False):
    assert payload[:4]==b'RIFF' and struct.unpack_from('<I',payload,4)[0]+8==len(payload)
    chunks={};offset=12
    while offset<len(payload):
        tag=payload[offset:offset+4];size=struct.unpack_from('<I',payload,offset+4)[0];end=offset+8+size
        assert end<=len(payload) and tag not in chunks
        chunks[tag]=payload[offset+8:end];offset=end+(size&1)
    assert offset==len(payload)
    fmt=struct.unpack('<HHIIHHHH',chunks[b'fmt ']);spb=1+(block_bytes//channels-4)*8//3
    assert fmt==(17,channels,8000,8000*block_bytes//spb,block_bytes,3,2,spb)
    assert struct.unpack('<I',chunks[b'fact'])[0]==frames
    assert len(chunks[b'data'])==((frames+spb-1)//spb)*block_bytes
    return decode_data(chunks[b'data'],channels,block_bytes,frames,combined=combined)
