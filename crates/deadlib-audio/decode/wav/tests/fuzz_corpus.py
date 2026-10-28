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

    def form(kind, fmt, audio, channels, frames, metadata=b'', trailing=False, fact=False, wavl=False):
        e = '>' if kind==b'RIFX' else '<'
        large = kind in (b'RF64',b'BW64')
        data = ck(b'data',audio,e,0xffffffff if large else None)
        if wavl:
            align=len(audio)//frames
            sequence=b'wavl'+ck(b'slnt',struct.pack(e+'I',3),e)+ck(b'data',audio,e)+ck(b'JUNK',b'odd',e)+ck(b'slnt',struct.pack(e+'I',5),e)+ck(b'data',b'',e)+ck(b'data',audio[:align],e)
            data=ck(b'LIST',sequence,e);frames+=9
        chunks = ck(b'fmt ',fmt,e)+metadata
        if fact:
            chunks += ck(b'fact',struct.pack(e+'I',min(frames,0xffffffff)),e)
        body = (data+chunks if trailing else chunks+data)+ck(b'zzzz',b'odd',e)
        if large:
            # Exercise repeated IDs and consumption of distinct ds64 table rows.
            body += ck(b'XTRA',b'a',size=0xffffffff)+ck(b'XTRA',b'bc',size=0xffffffff)
            table = b'XTRA'+struct.pack('<Q',1)+b'XTRA'+struct.pack('<Q',2)
            ds = ck(b'ds64',struct.pack('<QQQI',4+8+28+len(table)+len(body),0 if wavl else len(audio),frames,2)+table)
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
                result[name+'-wavl']=b'\0'+form(kind,fmt,audio,channels,7,info,fact=True,wavl=True)

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

    # Three-bit IMA codes cross byte/word boundaries. Channel words, rather than
    # whole 12-byte channel payloads, alternate physically (RIFFNEW pp.31-33).
    for kind in (b'RIFF',b'RIFX',b'RF64',b'BW64'):
        e='>' if kind==b'RIFX' else '<'
        for channels in (1,2,3,5):
            block=40*channels;frames=97
            fmt=struct.pack(e+'HHIIHHHH',17,channels,8000,8000*block//frames,block,3,2,frames)
            encoded=bytearray(block)
            for c in range(channels):
                struct.pack_into(e+'hBB',encoded,c*4,-32000+c*1234,20+c,0)
                for i in range(96):
                    code=(i*5+c*3)%8
                    for bit_in_code in range(3):
                        bit=i*3+bit_in_code
                        offset=4*channels+(bit//32*channels+c)*4+bit%32//8
                        encoded[offset]|=((code>>bit_in_code)&1)<<(bit%8)
            short=encoded[:16*channels]
            name=f'{kind.decode()}-ima3-{channels}'
            result[name]=b'\0'+form(kind,fmt,encoded*2+short,channels,frames*2+32,fact=True)
            result[name+'-block']=b'\2'+bytes([e=='>'])+struct.pack('<H',len(fmt))+fmt+encoded
            result[name+'-short']=b'\2'+bytes([e=='>'])+struct.pack('<H',len(fmt))+fmt+short
        for channels in (1,2,6):
            result[f'encode-ima3-{kind.decode()}-{channels}']=b'\3'+bytes([(b'RIFF',b'RIFX',b'RF64',b'BW64').index(kind),3,channels-1,2])+struct.pack('<I',7999)+bytes([2*channels-2,3])+bytes(range(256))

    # RF64's raw ds64 sample count replaces fact only for the 32-bit sentinel.
    # Keep fmt before audio and vary fact placement independently of metadata.
    for kind in (b'RF64',b'BW64'):
        for wavl in (False,True):
            if wavl:
                fmt=struct.pack('<HHIIHH',1,2,8000,32000,4,16)
                data=ck(b'LIST',b'wavl'+ck(b'slnt',struct.pack('<I',3))+ck(b'data',struct.pack('<4h',-32768,32767,-1,1))+ck(b'slnt',struct.pack('<I',4)))
                frames=9
            else:
                fmt=struct.pack('<HHIIHHHH',17,1,8000,8000*8//9,8,4,2,9)
                data=ck(b'data',struct.pack('<hBB',-12345,20,0)+b'\x12\x34\x56\x78')
                frames=7
            for trailing in (False,True):
                for count,sentinel in [(0,False),(1,False),(frames,False),(0xffffffffffffffff,False),(frames,True)]:
                    fact=ck(b'fact',struct.pack('<I',0xffffffff if sentinel else frames))
                    body=ck(b'fmt ',fmt)+(data+fact if trailing else fact+data)
                    ds=ck(b'ds64',struct.pack('<QQQI',4+36+len(body),0xffffffffffffffff,count,0))
                    result[f'duration-{kind.decode()}-{wavl}-{trailing}-{count}-{sentinel}']=b'\0'+kind+b'\xff'*4+b'WAVE'+ds+body

    # BW64 dummy words remain raw metadata even with absent/sentinel fact.
    for tag in (1,17):
        fmt=(struct.pack('<HHIIHH',1,2,8000,32000,4,16) if tag==1 else
             struct.pack('<HHIIHHHH',17,1,8000,32000,4,4,2,1))
        for fact_count in (None,1,0xffffffff):
            for dummy in (0,1,0xffffffffffffffff):
                fact=b'' if fact_count is None else ck(b'fact',struct.pack('<I',fact_count))
                body=ck(b'fmt ',fmt)+fact+ck(b'data',struct.pack('<hBB',12345,0,0))
                ds=ck(b'ds64',struct.pack('<QQQI',40+len(body),4,dummy,0))
                result[f'bw64-dummy-{tag}-{fact_count}-{dummy}']=b'\0BW64'+b'\xff'*4+b'WAVE'+ds+body

    # IMA's extensible Samples union carries block frames, not valid bit depth.
    for kind in (b'RIFF',b'RIFX',b'RF64',b'BW64'):
        e='>' if kind==b'RIFX' else '<';large=kind in (b'RF64',b'BW64')
        for bits in (3,4):
            for channels in (1,2,6,33):
                block_bytes=channels*(16 if bits==3 else 8);frames=33 if bits==3 else 9
                mask=(1<<channels)-1 if channels<32 else 0x80000000
                fmt=struct.pack(e+'HHIIHHHHIIHH',0xfffe,channels,8000,8000*block_bytes//frames,block_bytes,bits,22,frames,mask,17,0,16)+b'\x80\0\0\xaa\0\x38\x9b\x71'
                header=b''.join(struct.pack(e+'hBB',-30000+c*100,c%89,0) for c in range(channels))
                encoded=header+bytes((i*37+11)&255 for i in range(block_bytes-len(header)))
                name=f'ima-extensible-{kind.decode()}-{bits}-{channels}'
                result[name+'-block']=b'\2'+bytes([e=='>'])+struct.pack('<H',len(fmt))+fmt+encoded
                for trailing in (False,True):
                    fact=ck(b'fact',struct.pack(e+'I',frames-1),e);data=ck(b'data',encoded,e,0xffffffff if large else None)
                    body=ck(b'fmt ',fmt,e)+(data+fact if trailing else fact+data)
                    prefix=ck(b'ds64',struct.pack('<QQQI',40+len(body),len(encoded),frames-1 if kind==b'RF64' else 0,0)) if large else b''
                    result[name+f'-{trailing}']=b'\0'+kind+struct.pack(e+'I',0xffffffff if large else 4+len(body))+b'WAVE'+prefix+body

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
