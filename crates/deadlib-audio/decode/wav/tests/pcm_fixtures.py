"""Independent native-WAV fixtures: only struct packing and seeded integer PRNG.

Float fixtures are packed from IEEE bit words, never Python floating values.
Canonical integer truth is container-scale signed samples shifted to i32 bit 31.
"""
import hashlib
import random
import struct

KINDS={'u8':(1,8),'i16':(1,16),'i24':(1,24),'i32':(1,32),'f32':(3,32),'f64':(3,64)}


def chunk(tag,data,endian='<',sentinel=False,pad=0):
    return tag+struct.pack(endian+'I',0xffffffff if sentinel else len(data))+data+bytes([pad])*(len(data)&1)


def make(container,kind,channels,frames,valid=None,table=False,data_first=False,odd_metadata=False,pad=0):
    tag,bits=KINDS[kind];valid=bits if valid is None else valid
    extensible=channels>2 or valid!=bits
    name=f'{container.lower()}-{kind}-{channels}-{frames}-v{valid}'+('-table' if table else '')+('-data-first' if data_first else '')+('-odd-metadata' if odd_metadata else '')+('-nonzero-pad' if pad else '')
    endian='>' if container=='RIFX' else '<';large=container in ('RF64','BW64')
    seed=int.from_bytes(hashlib.sha256(name.encode()).digest()[:8],'little');rng=random.Random(seed)
    wire=bytearray();truth=bytearray()
    words32=[0,0x80000000,1,0x80000001,0x007fffff,0x00800000,0x7f7fffff,0xff7fffff,0x3f800000,0xbf800000,0x40000000,0xc0000000,0x7f800000,0xff800000,0x7fc12345,0xffc12345,0x7f812345,0xff812345]
    words64=[0,0x8000000000000000,1,0x8000000000000001,0x000fffffffffffff,0x0010000000000000,0x7fefffffffffffff,0xffefffffffffffff,0x3ff0000000000000,0xbff0000000000000,0x4000000000000000,0xc000000000000000,0x7ff0000000000000,0xfff0000000000000,0x7ff8123456789abc,0xfff8123456789abc,0x7ff0123456789abc,0xfff0123456789abc]
    for i in range(frames*channels):
        if tag==1:
            bound=1<<(valid-1);edges=sorted(set([-bound,-bound+1,-1,0,bound-2,bound-1]))
            edges=[n for n in edges if -bound<=n<bound]
            value=(edges[i] if i<len(edges) else rng.randint(-bound,bound-1))<<(bits-valid)
            raw=value+128 if bits==8 else value&((1<<bits)-1)
            wire.extend(raw.to_bytes(bits//8,'little' if endian=='<' else 'big'))
            truth.extend(struct.pack('<i',value<<(32-bits)))
        else:
            words=words32 if bits==32 else words64
            value=words[i] if i<len(words) else rng.getrandbits(bits)
            wire.extend(value.to_bytes(bits//8,'little' if endian=='<' else 'big'))
            truth.extend(value.to_bytes(bits//8,'little'))
    align=channels*bits//8;rate=192000 if valid<bits else 48000
    mask={1:4,2:3,6:0x3f}.get(channels,0) if extensible else 0
    fmt=struct.pack(endian+'HHIIHH',0xfffe if extensible else tag,channels,rate,rate*align,align,bits)
    if extensible:
        fmt+=struct.pack(endian+'HHIIHH',22,valid,mask,tag,0,0x10)+b'\x80\0\0\xaa\0\x38\x9b\x71'
    elif tag==3:fmt+=b'\0\0'
    leading=chunk(b'JUNK',b'odd\0\xff' if odd_metadata else b'even\0\xff',endian,pad=pad)+chunk(b'fmt ',fmt,endian,pad=pad)+chunk(b'LIST',b'INFO'+chunk(b'INAM',b'caf\xe9\0',endian,pad=pad),endian,pad=pad)
    if tag==3:leading+=chunk(b'fact',struct.pack(endian+'I',frames),endian,pad=pad)
    if table:leading+=chunk(b't64!',b'abc',endian,sentinel=True,pad=pad)
    data=chunk(b'data',wire,endian,sentinel=large,pad=pad)
    body=b'WAVE'+(data+leading if data_first else leading+data)+chunk(b'zzzz',b'trailing\0\xfe',endian,pad=pad)
    if large:
        rows=b't64!'+struct.pack('<Q',3) if table else b''
        ds=struct.pack('<QQQI',len(body)+36+len(rows),len(wire),frames,int(table))+rows
        payload=container.encode()+b'\xff'*4+b'WAVE'+chunk(b'ds64',ds)+body[4:]
    else:payload=container.encode()+struct.pack(endian+'I',len(body))+body
    info=dict(name=name,container=container,kind=kind,channels=channels,frames=frames,bits=bits,valid_bits=valid,mask=mask,sample_rate=rate,extensible=extensible,ds64_table=table,data_first=data_first,odd_metadata=odd_metadata,pad_byte=pad,profile='compatibility' if data_first or pad else 'canonical',seed=seed,expected_bytes=len(truth),canonical='s32le_left_aligned' if tag==1 else kind+'le_native_bits',core_reference_gate=container=='RIFF' and channels<=2 and valid==bits and frames>0 and not data_first and not odd_metadata and not pad)
    return info,payload,bytes(truth)


def cases():
    for container in ('RIFF','RIFX','RF64','BW64'):
        for kind in KINDS:
            for channels in (1,2,6,33):
                for frames in (0,17,521):yield make(container,kind,channels,frames)
        for bits,valid in ((8,3),(16,12),(24,20),(24,23),(32,24),(32,1)):
            for channels in (1,2,6):yield make(container,{8:'u8',16:'i16',24:'i24',32:'i32'}[bits],channels,37,valid)
    for container in ('RF64','BW64'):
        for kind in ('u8','f64'):
            for channels in (1,6):yield make(container,kind,channels,17,table=True)
    for container in ('RIFF','RIFX'):
        for kind in KINDS:yield make(container,kind,1,17,data_first=True)
    for container in ('RIFF','RIFX','RF64','BW64'):
        for kind in KINDS:
            for channels in (1,2,6):yield make(container,kind,channels,17,odd_metadata=True)
        for kind in ('u8','f64'):yield make(container,kind,1,17,odd_metadata=True,pad=0xa5)
