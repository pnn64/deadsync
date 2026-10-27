"""Independent bit-written intensity/MS stereo streams; no encoder algorithms.

Band boundaries come from primary numeric fixtures, not native source. All three
public reference decoders consume the same bytes. Includes each pan position,
uncoded top bands, per-window right-channel bounds, and mixed long/short bounds.
"""
import argparse, hashlib, json
from pathlib import Path
from reference_huffman import Bits


def ordering(bands, shape, mpeg1):
    long, short = bands
    mixed = shape == 'mixed'
    count = (8 if mpeg1 else 6) if mixed else 22 if shape == 'long' else 0
    result = [(n, -1, f) for n in range(count) for f in range(long[n], long[n+1])]
    if shape != 'long':
        result += [(n, w, 3*f+w) for n in range(3 if mixed else 0, 13)
                   for w in range(3) for f in range(short[n], short[n+1])]
    assert len(result) == 576 and sorted(f for _, _, f in result) == list(range(576))
    return result


def granule(mpeg1, shape, pan, scale, left_line, right_line, order):
    parts=[];infos=[]
    for channel, line in enumerate([left_line, right_line]):
        part=Bits()
        if channel:
            count=21 if shape=='long' else 36 if shape=='short' else 35 if mpeg1 else 33
            for _ in range(count):part.put(pan,3)
        big = 0 if line is None else line//2+1
        # Normative book 1: 00=1, 10=01, 01=001, 11=000; sign follows each nonzero.
        for pair in range(big):
            x=int(2*pair==line);y=int(2*pair+1==line)
            part.put(1, 1 if not x and not y else 2 if x else 3)
            if x or y:part.put(0,1)
        parts.append(part)
        infos.append((len(part.bits),big,13 if mpeg1 and channel else 258+scale if channel else 0))
    return parts,infos


def frame(rate, bands, shape, mode, scale, case, empty=False):
    mpeg1=rate>=32000;version=3 if mpeg1 else 2 if rate>=16000 else 0
    base=[44100,48000,32000];div=1 if mpeg1 else 2 if version==2 else 4
    index=[n//div for n in base].index(rate)
    bitrate=320000 if mpeg1 else 160000
    size=(144 if mpeg1 else 72)*bitrate//rate
    order=ordering(bands,shape,mpeg1)
    side=Bits();side.put(0,9 if mpeg1 else 8);side.put(0,3 if mpeg1 else 2)
    if mpeg1:
        side.put(0,4);side.put(0,4)
    payload=Bits()
    for gran in range(2 if mpeg1 else 1):
        pan,left,right=case if gran==0 and not empty else (7,None,None)
        parts,infos=granule(mpeg1,shape,pan,scale,left,right,order)
        for channel,(length,big,compress) in enumerate(infos):
            side.put(length,12);side.put(big,9);side.put(206,8)
            side.put(compress,4 if mpeg1 else 9)
            switched=shape!='long';side.put(switched,1)
            if switched:
                side.put(2,2);side.put(shape=='mixed',1)
                side.put(1,5);side.put(1,5)
                for _ in range(3):side.put(0,3)
            else:
                for _ in range(3):side.put(1,5)
                side.put(0,4);side.put(0,3)
            if mpeg1:side.put(0,1)
            side.put(0,1);side.put(0,1)
            payload.bits.extend(parts[channel].bits)
    assert len(side.bits)==(256 if mpeg1 else 136)
    header=(0x7ff<<21)|(version<<19)|(1<<17)|(1<<16)|(14<<12)|(index<<10)|(1<<6)|(mode<<4)
    data=header.to_bytes(4,'big')+side.data()+payload.data()
    assert len(data)<=size,(rate,shape,len(data),size)
    return data+bytes(size-len(data))


def main():
    p=argparse.ArgumentParser();p.add_argument('--bands',type=Path,required=True)
    p.add_argument('--output',type=Path,required=True);p.add_argument('--smoke',action='store_true')
    args=p.parse_args();args.output.mkdir(parents=True,exist_ok=False)
    bands=json.loads(args.bands.read_text());records=[]
    for rate_text, tables in bands['rates'].items():
        rate=int(rate_text)
        if args.smoke and rate!=44100:continue
        for shape in ['long'] if args.smoke else ['long','short','mixed']:
            order=ordering(tables,shape,rate>=32000)
            groups={}
            for n,(band,window,_) in enumerate(order):groups.setdefault((band,window),[]).append(n)
            for mode in [1,3]:
                for scale in range(1 if rate>=32000 else 2):
                    cases=[]
                    for (band,window),lines in groups.items():
                        if args.smoke and band!=0:continue
                        for pan in range(8):
                            cases.append((pan,lines[0],None))
                        # Right data in this band blocks IS here; data below preserves it.
                        cases.append((3,lines[-1],lines[0]))
                        if lines[0]:cases.append((3,lines[0],lines[0]-1))
                    name=f'{rate}-{shape}-{mode}-{scale}.mp3';path=args.output/name
                    with path.open('wb') as out:
                        for case in cases:out.write(frame(rate,tables,shape,mode,scale,case))
                        for _ in range(2):out.write(frame(rate,tables,shape,mode,scale,(7,None,None),True))
                    records.append({'file':name,'frames':len(cases)+2,'cases':cases,'sha256':hashlib.sha256(path.read_bytes()).hexdigest()})
    (args.output/'generation.json').write_text(json.dumps({'method':__doc__,'bands_sha256':hashlib.sha256(args.bands.read_bytes()).hexdigest(),'records':records},indent=2)+'\n')
    print('files',len(records),'frames',sum(r['frames'] for r in records))


if __name__=='__main__':main()
