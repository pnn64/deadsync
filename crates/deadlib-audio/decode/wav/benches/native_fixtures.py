"""Fixed native benchmark matrix, independent packing and bit-word truth periods."""
import hashlib
from pathlib import Path
import random
import struct

KINDS={'u8':(1,8),'i16':(1,16),'i24':(1,24),'i32':(1,32),'f32':(3,32),'f64':(3,64)}


def chunk(tag,b,e='<',sentinel=False):return tag+struct.pack(e+'I',0xffffffff if sentinel else len(b))+b+b'\0'*(len(b)&1)


def make(directory,container,kind,channels,valid=None,metadata=0,offset=0,samples=1<<20,suffix=''):
    tag,bits=KINDS[kind];valid=valid or bits;frames=max(1,samples//channels);samples=frames*channels
    name=f'{container}-{kind}-c{channels}-v{valid}-m{metadata}-o{offset}'+suffix
    e='>' if container=='RIFX' else '<';large=container in ('RF64','BW64');ext=channels>2 or valid!=bits
    rng=random.Random(0x4e415456+bits*73+valid*19+channels);wire=bytearray();truth=bytearray()
    floats={32:[0,0x80000000,1,0x007fffff,0x00800000,0x7f7fffff,0xff7fffff,0x3f800000,0xbf800000,0x7f800000,0xff800000,0x7fc12345,0xff812345],64:[0,0x8000000000000000,1,0x000fffffffffffff,0x0010000000000000,0x7fefffffffffffff,0xffefffffffffffff,0x3ff0000000000000,0xbff0000000000000,0x7ff0000000000000,0xfff0000000000000,0x7ff8123456789abc,0xfff0123456789abc]}
    for i in range(4096):
        if tag==1:
            bound=1<<(valid-1);edges=[-bound,-bound+1,-1,0,bound-2,bound-1]
            value=(edges[i] if i<len(edges) else rng.randrange(-bound,bound))<<(bits-valid)
            raw=value+128 if bits==8 else value&((1<<bits)-1);truth.extend(struct.pack('<i',value))
        else:
            raw=floats[bits][i] if i<len(floats[bits]) else rng.getrandbits(bits);truth.extend(raw.to_bytes(bits//8,'little'))
        wire.extend(raw.to_bytes(bits//8,'big' if e=='>' else 'little'))
    width=bits//8;audio=(wire*(samples//4096)+wire[:samples%4096*width]);mask=0x3f if channels==6 else 0
    fmt=struct.pack(e+'HHIIHH',0xfffe if ext else tag,channels,48000,48000*channels*width,channels*width,bits)
    if ext:fmt+=struct.pack(e+'HHIIHH',22,valid,mask,tag,0,16)+b'\x80\0\0\xaa\0\x38\x9b\x71'
    elif tag==3:fmt+=b'\0\0'
    leading=b''.join(chunk(b'JUNK',bytes([i%256])*32,e) for i in range(metadata))+chunk(b'fmt ',fmt,e)
    if tag==3:leading+=chunk(b'fact',struct.pack(e+'I',frames),e)
    # Ensure data is eight-byte aligned relative to the form, before an optional
    # deliberately unaligned in-memory base is applied by the Rust harness.
    pad=(-(12+(36 if large else 0)+len(leading)+8))%8
    if pad:leading+=chunk(b'JUNK',b'\0'*pad,e)
    body=leading+chunk(b'data',audio,e,large)+chunk(b'last',b'footer',e)
    prefix=chunk(b'ds64',struct.pack('<QQQI',40+len(body),len(audio),frames if container=='RF64' else 0,0)) if large else b''
    payload=container.encode()+struct.pack(e+'I',0xffffffff if large else 4+len(body))+b'WAVE'+prefix+body
    path=directory/(name+'.wav');period=directory/(name+'.period');path.write_bytes(payload);period.write_bytes(truth)
    data_offset=12+len(prefix)+len(leading)+8
    sections={'header':payload[:data_offset],'wire':bytes(wire),'tail':payload[data_offset+len(audio):],'period':bytes(truth)}
    for label,data in sections.items():(directory/(name+'.'+label)).write_bytes(data)
    return dict(name=name,container=container,kind=kind,channels=channels,frames=frames,samples=samples,bits=bits,valid_bits=valid,metadata_chunks=metadata,memory_offset=offset,wire_bytes=len(audio),file_bytes=len(payload),data_offset=data_offset,fixture_sha256=hashlib.sha256(payload).hexdigest(),period_sha256=hashlib.sha256(truth).hexdigest(),sections={label:hashlib.sha256(data).hexdigest() for label,data in sections.items()},hound_decode_gate=container=='RIFF' and valid==bits and kind!='f64')


def matrix(directory):
    rows=[]
    for kind in KINDS:
        for channels in (1,2,6):rows.append(make(directory,'RIFF',kind,channels))
    for container in ('RIFX','RF64','BW64'):
        for kind in KINDS:rows.append(make(directory,container,kind,2))
    for kind,valid in (('u8',3),('i16',12),('i24',20),('i32',24)):rows.append(make(directory,'RIFF',kind,2,valid=valid))
    for kind in KINDS:
        rows.append(make(directory,'RIFF',kind,2,metadata=128));rows.append(make(directory,'RIFF',kind,2,offset=1))
    return rows
