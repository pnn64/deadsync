"""Small independently packed corpus for the shared WAV property harness.

Every entry starts with a harness selector. This is not a production file format.
No production codec/writer or external reference creates these seed bytes.
"""
import hashlib
import struct


def corpus():
    result = {}

    def ck(tag, data, e='<', size=None):
        return tag+struct.pack(e+'I',len(data) if size is None else size)+data+b'\xa5'*(len(data)&1)

    def form(kind, fmt, audio, channels, frames, metadata=b'', trailing=False, fact=False):
        e = '>' if kind==b'RIFX' else '<'
        large = kind in (b'RF64',b'BW64')
        chunks = ck(b'fmt ',fmt,e)+metadata
        if fact:
            chunks += ck(b'fact',struct.pack(e+'I',frames),e)
        data = ck(b'data',audio,e,0xffffffff if large else None)
        body = (data+chunks if trailing else chunks+data)+ck(b'zzzz',b'odd',e)
        if large:
            # Exercise repeated IDs and consumption of distinct ds64 table rows.
            body += ck(b'XTRA',b'a',size=0xffffffff)+ck(b'XTRA',b'bc',size=0xffffffff)
            table = b'XTRA'+struct.pack('<Q',1)+b'XTRA'+struct.pack('<Q',2)
            ds = ck(b'ds64',struct.pack('<QQQI',4+8+28+len(table)+len(body),len(audio),frames,2)+table)
            return kind+b'\xff'*4+b'WAVE'+ds+body
        return kind+struct.pack(e+'I',4+len(body))+b'WAVE'+body

    for kind in (b'RIFF',b'RIFX',b'RF64',b'BW64'):
        e = '>' if kind==b'RIFX' else '<'
        for tag,bits in ((1,8),(1,16),(1,24),(1,32),(3,32),(3,64)):
            for channels in (1,2,3,33):
                align=channels*bits//8
                fmt=struct.pack(e+'HHIIHH',tag,channels,48000,48000*align,align,bits)
                if channels>2:
                    fmt=struct.pack(e+'H',0xfffe)+fmt[2:]+struct.pack(e+'HHI',22,bits-(3 if tag==1 else 0),0)+struct.pack(e+'IHH',tag,0,0x10)+b'\x80\0\0\xaa\0\x38\x9b\x71'
                elif tag==3:
                    fmt+=b'\0\0'
                # All byte patterns, including low unused valid bits, remain significant here.
                audio=bytes((i*73+181)&255 for i in range(align*7))
                if tag==3:
                    values=([0,0x80000000,0x7f800000,0xff800000,0x7fc12345,0x7f812345,0x00000001] if bits==32 else
                            [0,0x8000000000000000,0x7ff0000000000000,0xfff0000000000000,0x7ff8123456789abc,0x7ff0123456789abc,1])
                    audio=b''.join(struct.pack(e+('I' if bits==32 else 'Q'),v) for v in values for _ in range(channels))
                name=f'{kind.decode()}-{tag}-{bits}-{channels}'
                info=ck(b'LIST',b'INFO'+ck(b'INAM',b'caf\xe9\0',e),e)
                result[name]=b'\0'+form(kind,fmt,audio,channels,7,info,trailing=channels%2==1,fact=tag==3)

    coef=[(256,0),(512,-256),(0,0),(192,64),(240,0),(460,-208),(392,-232)]
    for e in ('<','>'):
        for tag in (2,17):
            for channels in ((1,2) if tag==2 else (1,2,3,5)):
                block=channels*(32 if tag==17 else 31)
                frames=(1 if tag==17 else 2)+(block-channels*(4 if tag==17 else 7))*2//channels
                extra=struct.pack(e+'H',frames)
                if tag==2:
                    extra+=struct.pack(e+'H',len(coef))+b''.join(struct.pack(e+'hh',*row) for row in coef)
                fmt=struct.pack(e+'HHIIHHH',tag,channels,44100,44100*block//frames,block,4,len(extra))+extra
                if tag==17:
                    header=b''.join(struct.pack(e+'hBB',-32000+c*1234,20+c,0) for c in range(channels))
                else:
                    header=bytes(range(channels))+struct.pack(e+'h',16)*channels+struct.pack(e+'h',-1000)*channels+struct.pack(e+'h',1000)*channels
                encoded=header+bytes((i*37+11)&255 for i in range(block-len(header)))
                name=f'adpcm-{"le" if e=="<" else "be"}-{tag}-{channels}'
                result[name]=b'\0'+form(b'RIFF' if e=='<' else b'RIFX',fmt,encoded*2,channels,frames*2-1,fact=True,trailing=True)
                result[name+'-block']=b'\2'+bytes([e=='>'])+struct.pack('<H',len(fmt))+fmt+encoded

    metadata=[]
    for e in ('<','>'):
        metadata.extend([
            (0,e,b'adtl'+ck(b'labl',struct.pack(e+'I',7)+b'label\0',e)+ck(b'ltxt',struct.pack(e+'II4s4H',7,19,b'rgn ',44,9,1,1252)+b'region\0',e)),
            (1,e,struct.pack(e+'I6I',1,7,0,int.from_bytes(b'data','little' if e=='<' else 'big'),0,0,19)),
            (2,e,struct.pack(e+'9I6I',19,17,20833,60,0x80000000,25,23,1,3,7,0,1,19,0x40000000,2)+b'\1\2\3'),
        ])
    for version in (0,1,2,0xffff):
        bext=bytearray(602)
        bext[:5]=b'audio';bext[320:338]=b'2026-10-2812:00:00'
        struct.pack_into('<QH',bext,338,0x123456789abcdef0,version)
        if version>=1: bext[348:412]=bytes(range(64))
        if version>=2: struct.pack_into('<5h',bext,412,-2300,1200,-100,-1800,32767)
        metadata.append((3,'<',bytes(bext)+b'A=PCM\r\n'))
    chna=struct.pack('<HH',2,3)
    for track,uid,ref in ((1,1,b'AT_00010001_01'),(1,2,b'AC_00010002_00'),(2,3,b'AT_00010003_01')):
        chna+=struct.pack('<H',track)+f'ATU_{uid:08x}'.encode()+ref+b'AP_00010001'+b'\0'
    chna+=b'\0'*40
    metadata.extend([(4,'<',chna),(5,'<',b'\0\0<adm/>'),(5,'<',b'\1\0\x1f\x8b\x08\0opaque')])
    sxml=struct.pack('<HQI',0,4+8+5+8+6,2)+struct.pack('<II',5,1024)+b'<a/ >'+struct.pack('<II',6,2048)+b'<b />!'
    metadata.extend([(6,'<',sxml),(6,'<',sxml+struct.pack('<IQQQQ',2,14,0,27,0x100000001))])
    marker=bytearray(320)
    struct.pack_into('<IQQII',marker,0,0x17,0x100000002,0x300000004,0x12345678,0xabcdef01)
    marker[28:35]=b'marker\0';marker[288:304]=bytes(range(16));struct.pack_into('<4I',marker,304,1,2,3,0xffffffff)
    metadata.extend([(7,'<',bytes(marker)),(8,'<',b'<audioFormatExtended/>'),(9,'<',b'unknown\0\xff')])
    for i,(kind,e,payload) in enumerate(metadata):
        result[f'metadata-{i}-{kind}']=b'\1'+bytes([kind,e=='>'])+payload
    for container in range(4):
        for sample in range(6):
            result[f'encode-{container}-{sample}']=b'\3'+bytes([container,3,2,sample])+struct.pack('<I',47999)+b'\x20\x03'+bytes(range(256))
    return result


def write_corpus(directory):
    directory.mkdir(parents=True,exist_ok=False)
    seeds=corpus()
    for name,data in seeds.items():
        (directory/name).write_bytes(data)
    return {name:hashlib.sha256(data).hexdigest() for name,data in sorted(seeds.items())}


if __name__=='__main__':
    import argparse
    from pathlib import Path
    ap=argparse.ArgumentParser();ap.add_argument('directory',type=Path)
    args=ap.parse_args()
    print(f'{len(write_corpus(args.directory))} independent seeds written')
