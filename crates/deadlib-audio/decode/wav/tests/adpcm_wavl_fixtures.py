"""Independent compressed wavl packing, natural-order timeline and scalar truth."""
import random
import struct
from ima_extensible_fixtures import descriptor as ima_format,block as ima_block,decode_block as ima_decode,chunk,wave as flat_wave

COEFFICIENTS=((256,0),(512,-256),(0,0),(192,64),(240,0),(460,-208),(392,-232))
ADAPT=(230,230,230,230,307,409,512,614,768,614,512,409,307,230,230,230)


def ms_format(channels,block_bytes,e):
    frames=2+(block_bytes-7*channels)*2//channels
    return struct.pack(e+'HHIIHHHHH14h',2,channels,8000,8000*block_bytes//frames,block_bytes,4,32,frames,7,*[v for pair in COEFFICIENTS for v in pair])


def ms_decode(payload,channels,e):
    planes=[]
    for c in range(channels):
        a,b=COEFFICIENTS[payload[c]]
        delta=struct.unpack_from(e+'h',payload,channels+2*c)[0]
        one=struct.unpack_from(e+'h',payload,3*channels+2*c)[0]
        two=struct.unpack_from(e+'h',payload,5*channels+2*c)[0]
        plane=[two,one]
        for i in range((len(payload)-7*channels)*2//channels):
            at=i*channels+c
            code=(payload[7*channels+at//2]>>(4 if at%2==0 else 0))&15
            product=one*a+two*b
            prediction=abs(product)//256*(-1 if product<0 else 1)
            value=max(-32768,min(32767,prediction+delta*(code if code<8 else code-16)))
            two,one=one,value
            delta=max(16,delta*ADAPT[code]//256)
            plane.append(value)
        planes.append(plane)
    return [planes[c][f] for f in range(len(planes[0])) for c in range(channels)]


def form(container,fmt,list_payload,frames,trailing=False):
    e='>' if container=='RIFX' else '<';large=container in ('RF64','BW64')
    fact=chunk(b'fact',struct.pack(e+'I',min(frames,0xffffffff)),e)
    audio=chunk(b'LIST',list_payload,e,large)
    body=chunk(b'fmt ',fmt,e)+chunk(b'JUNK',b'odd',e)+(audio+fact if trailing else fact+audio)
    prefix=b''
    if large:
        ds=struct.pack('<QQQI4sQ',52+len(body),0,frames if container=='RF64' else 0xffffffffffffffff,1,b'LIST',len(list_payload))
        prefix=chunk(b'ds64',ds,e)
    return container.encode()+struct.pack(e+'I',0xffffffff if large else 4+len(body))+b'WAVE'+prefix+body


def cases():
    for container in ('RIFF','RIFX','RF64','BW64'):
        e='>' if container=='RIFX' else '<'
        for bits in (0,3,4):
            for channels in ((1,2) if bits==0 else (1,2,6,33)):
                for extensible in ((False,) if bits==0 else (False,True)):
                    rng=random.Random(0x5741564c+bits*19+channels*173)
                    block_bytes=channels*(23 if bits==0 else 28 if bits==3 else 12)
                    fmt=ms_format(channels,block_bytes,e) if bits==0 else ima_format(bits,channels,block_bytes,3 if channels==2 else 0,e,extensible)
                    def block(groups,n):
                        if bits:
                            headers=[(((n*3917+c*1777)%65536)-32768,(n*17+c*19)%89) for c in range(channels)]
                            codes=[[rng.randrange(1<<bits) for _ in range(groups*(32 if bits==3 else 8))] for c in range(channels)]
                            raw=ima_block(bits,headers,codes,e)
                            return raw,ima_decode(bits,raw,channels,e),ima_decode(bits,raw,channels,e,True)
                        raw=bytes((n+c)%7 for c in range(channels))
                        raw+=struct.pack(e+'h'*channels,*[16+c*13 for c in range(channels)])
                        raw+=struct.pack(e+'h'*channels,*[-12345+n*971+c*1777 for c in range(channels)])
                        raw+=struct.pack(e+'h'*channels,*[23123-n*971-c*1777 for c in range(channels)])
                        raw+=bytes(rng.randrange(256) for _ in range(groups*channels))
                        truth=ms_decode(raw,channels,e)
                        return raw,truth,truth
                    full=block(16 if bits==0 else 2,0)
                    short=block(5 if bits==0 else 1,1)
                    header=block(0,2)
                    last=block(16 if bits==0 else 2,3)
                    children=[(b'slnt',2),(b'JUNK',b'odd'),(b'data',(full[0]+short[0],full[1]+short[1],full[2]+short[2])),
                              (b'slnt',3),(b'data',(b'',[],[])),(b'slnt',0),(b'JUNK',b''),(b'slnt',4),
                              (b'data',header),(b'slnt',2),(b'data',(last[0]*2,last[1]*2,last[2]*2)),(b'slnt',5)]
                    payload=b'wavl';truth=[];combined=[];boundaries=[]
                    for tag,value in children:
                        if tag==b'slnt':
                            payload+=chunk(tag,struct.pack(e+'I',value),e)
                            truth+=(truth[-channels:] if truth else [0]*channels)*value
                            combined+=(combined[-channels:] if combined else [0]*channels)*value
                        elif tag==b'data':
                            raw,decoded,alternate=value;payload+=chunk(tag,raw,e);truth+=decoded;combined+=alternate
                        else:payload+=chunk(tag,value,e)
                        boundaries.append(len(truth)//channels)
                    total=len(truth)//channels
                    for mode,frames in (('empty',0),('full',total),('trim',total-3)):
                        info=dict(name=f'{container}-{bits}-{channels}-ext{int(extensible)}-{mode}',container=container,bits=bits,channels=channels,extensible=extensible,mode=mode,frames=frames,capacity=total,children=len(children),block_bytes=block_bytes,boundaries=boundaries,trailing=mode=='trim')
                        packed=lambda s:struct.pack('<'+'h'*len(s),*s)
                        legacy=ms_format(channels,block_bytes,e) if bits==0 else ima_format(bits,channels,block_bytes,0,e,False)
                        paired=flat_wave(container,legacy,full[0],len(full[1])//channels)
                        yield info,form(container,fmt,payload,frames,info['trailing']),packed(truth[:frames*channels]),packed(combined[:frames*channels]),paired,packed(full[1]),packed(full[2])
