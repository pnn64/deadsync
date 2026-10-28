"""Independent periods for native/Symphonia comparison; no production encoders."""
import hashlib
import math
from pathlib import Path
import random
import struct
import sys
from native_fixtures import make as native_make, matrix as native_matrix

sys.path.insert(0,str(Path(__file__).resolve().parents[1]/'tests'))
from adpcm_wavl_fixtures import COEFFICIENTS, ADAPT, ms_format, ms_decode
from ima_extensible_fixtures import descriptor, block as ima_block, decode_block as ima_decode, chunk


def sha(data):return hashlib.sha256(data).hexdigest()


def ms_period(channels,e):
    rng=random.Random(0x50455246+channels);size=256*channels;frames=500
    blocks=[];truth=[]
    for n in range(16):
        predictor=[(n+c)%7 for c in range(channels)]
        delta=[rng.randrange(16,1024) for _ in range(channels)]
        one=[rng.randrange(-32768,32768) for _ in range(channels)]
        two=[rng.randrange(-32768,32768) for _ in range(channels)]
        header=bytes(predictor)+b''.join(struct.pack(e+'h'*channels,*values) for values in (delta,one,two))
        codes=[]
        # Quantize independent random targets to keep adaptation bounded, as a
        # real encoder would. Scalar decode below determines the wire truth.
        for _ in range(frames-2):
            for c in range(channels):
                a,b=COEFFICIENTS[predictor[c]];product=one[c]*a+two[c]*b
                estimate=abs(product)//256*(-1 if product<0 else 1)
                diff=rng.randrange(-32768,32768)-estimate
                code=max(-8,min(7,(abs(diff)+delta[c]//2)//delta[c]*(-1 if diff<0 else 1)))
                one[c],two[c]=max(-32768,min(32767,estimate+delta[c]*code)),one[c]
                delta[c]=max(16,delta[c]*ADAPT[code&15]//256);codes.append(code&15)
        raw=header+bytes((a<<4)|b for a,b in zip(codes[::2],codes[1::2]))
        assert len(raw)==size;blocks.append(raw);truth.extend(ms_decode(raw,channels,e))
    return b''.join(blocks),struct.pack('<'+'h'*len(truth),*truth),size,frames


def ima_period(bits,channels,sign_only,e):
    rng=random.Random(0x494d4150+bits*31+channels);groups=21 if bits==3 else 63
    frames=1+groups*(32 if bits==3 else 8);blocks=[];truth=[]
    for n in range(16):
        headers=[(rng.randrange(-32768,32768),(n*19+c*23)%89) for c in range(channels)]
        # Sign-only 4-bit codes have identical separate-shift and combined
        # arithmetic. Random 4-bit cases retain the actual rounding mismatch.
        codes=[[rng.choice((0,8)) if sign_only else rng.randrange(1<<bits) for _ in range(frames-1)] for c in range(channels)]
        raw=ima_block(bits,headers,codes,e);blocks.append(raw)
        truth.extend(ima_decode(bits,raw,channels,e))
    wire=b''.join(blocks);return wire,struct.pack('<'+'h'*len(truth),*truth),len(blocks[0]),frames


def compressed(directory,container,kind,channels,extensible=False,suffix='',wire_target=None,small=False):
    e='>' if container=='RIFX' else '<';large=container in ('RF64','BW64')
    if kind=='ms':wire,truth,block_bytes,block_frames=ms_period(channels,e);bits=4;fmt=ms_format(channels,block_bytes,e)
    else:
        bits=3 if kind=='ima3' else 4
        wire,truth,block_bytes,block_frames=ima_period(bits,channels,kind=='ima4sign',e)
        fmt=descriptor(bits,channels,block_bytes,0,e,extensible,rate=8000)
    period_frames=len(truth)//(channels*2)
    repetitions=math.ceil(wire_target/len(wire)) if wire_target else 1 if small else math.ceil((1<<20)/(period_frames*channels))
    frames=period_frames*repetitions;audio_bytes=len(wire)*repetitions
    leading=chunk(b'fmt ',fmt,e)+chunk(b'fact',struct.pack(e+'I',frames),e)+chunk(b'JUNK',b'odd',e)
    tail=b'\0'*(audio_bytes&1)+chunk(b'last',b'footer',e)
    body_bytes=len(leading)+8+audio_bytes+len(tail)
    ds=chunk(b'ds64',struct.pack('<QQQI',40+body_bytes,audio_bytes,frames if container=='RF64' else 0,0)) if large else b''
    header=container.encode()+struct.pack(e+'I',0xffffffff if large else 4+body_bytes)+b'WAVE'+ds+leading+b'data'+struct.pack(e+'I',0xffffffff if large else audio_bytes)
    name=f'{container}-{kind}-c{channels}-ext{int(extensible)}{suffix}'
    sections=dict(header=header,wire=wire,tail=tail,period=truth);digest=hashlib.sha256()
    with (directory/(name+'.wav')).open('wb') as f:
        f.write(header);digest.update(header)
        for _ in range(repetitions):f.write(wire);digest.update(wire)
        f.write(tail);digest.update(tail)
    for label,data in sections.items():(directory/(name+'.'+label)).write_bytes(data)
    return dict(name=name,container=container,kind=kind,channels=channels,frames=frames,samples=frames*channels,bits=bits,valid_bits=bits,metadata_chunks=1,memory_offset=0,wire_bytes=audio_bytes,file_bytes=len(header)+audio_bytes+len(tail),data_offset=len(header),fixture_sha256=digest.hexdigest(),period_sha256=sha(truth),sections={label:sha(b) for label,b in sections.items()},wire_repetitions=repetitions,wire_tail_bytes=0,block_bytes=block_bytes,block_frames=block_frames,extensible=extensible)


def native_case(row):
    row['wire_repetitions']=row['samples']//4096
    row['wire_tail_bytes']=(row['samples']%4096)*row['bits']//8
    return row


def matrix(directory):
    rows=[native_case(r) for r in native_matrix(directory)]
    for container in ('RIFF','RIFX','RF64','BW64'):
        for kind in ('ms','ima4','ima4sign','ima3'):
            for channels in (1,2):rows.append(compressed(directory,container,kind,channels))
    for kind,extensible in (('ima4',False),('ima4sign',False),('ima4sign',True),('ima3',False),('ima3',True)):
        rows.append(compressed(directory,'RIFF',kind,6,extensible))
    return rows


def memory_matrix(directory,small_only=False):
    rows=[]
    for kind,bits in (('u8',8),('i16',16),('i24',24),('i32',32),('f32',32),('f64',64)):
        for label,samples in (('small',8192),('large',(64<<20)//(bits//8))):
            if small_only and label=='large':continue
            rows.append(native_case(native_make(directory,'RIFF',kind,2,samples=samples,suffix='-rss-'+label)))
    for kind in ('ms','ima4sign'):
        for label in ('small','large'):
            if small_only and label=='large':continue
            rows.append(compressed(directory,'RIFF',kind,2,suffix='-rss-'+label,wire_target=(64<<20) if label=='large' else None,small=label=='small'))
    return rows
