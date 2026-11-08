"""All-frequency isolated-coefficient DSP checks through libmad's public API.

Native kernels receive explicitly reordered spectra. Reference MP3 frames encode
the same coefficients, with independent bit writing. Frames form a continuous
stateful sequence; each first granule is followed by a zero granule. Constructed
window transitions exercise equations and history, not full ISO conformance.
"""
import argparse,hashlib,json,math,struct,subprocess
from pathlib import Path
from reference_huffman import Bits

LONG=[0,4,8,12,16,20,24,30,36,44,52,62,74,90,110,134,162,196,238,288,342,418,576]
SHORT=[0,4,8,12,16,22,30,40,52,66,84,106,136,192]

def ordering(shape):
    if shape not in [2,4]:return list(range(576))
    mixed=shape==4
    result=list(range(36)) if mixed else []
    for band in range(3 if mixed else 0,13):
        for window in range(3):result.extend(3*line+window for line in range(SHORT[band],SHORT[band+1]))
    assert len(result)==576 and sorted(result)==list(range(576))
    return result

def encode(shape,line,negative):
    order=ordering(shape);coded=order.index(line);pairs=coded//2+1
    main=Bits()
    for _ in range(pairs-1):main.put(1,1) # codebook 1: (0,0)
    main.put(1,3 if coded%2 else 2) # (0,1)=001 or (1,0)=01
    main.put(int(negative),1)
    side=Bits();side.put(0,9);side.put(0,5);side.put(0,4)
    for granule in range(2):
        kind=[0,1,2,3,2,1,3][shape]
        if granule:kind=0
        for value,width in [(len(main.bits) if not granule else 0,12),(pairs if not granule else 0,9),(206 if not granule else 0,8),(0,4),(int(kind!=0),1)]:side.put(value,width)
        if kind:
            side.put(kind,2);side.put(int(shape>=4),1)
            side.put(1,5);side.put(1,5)
            for _ in range(3):side.put(0,3)
        else:
            for _ in range(3):side.put(1 if not granule else 0,5)
            side.put(0,4);side.put(0,3)
        side.put(0,1);side.put(0,1);side.put(0,1)
    assert len(side.bits)==136
    frame=bytearray(b'\xff\xfb\xe0\xc0')+side.data()+main.data()
    frame.extend(bytes(1044-len(frame)));return frame

def main():
    p=argparse.ArgumentParser()
    for name in ['native','oracle','output']:p.add_argument('--'+name,type=Path,required=True)
    p.add_argument('--quick',action='store_true');args=p.parse_args();args.output.mkdir(parents=True,exist_ok=False)
    selected=[0,1,17,18,35,36,71,72,127,255,511,575] if args.quick else range(576)
    cases=[(shape,line) for shape in range(7) for line in selected];count=len(cases)*2
    stream=args.output/'vectors.mp3';spectra=args.output/'spectra.bin'
    with stream.open('wb') as bitstream,spectra.open('wb') as target:
        target.write(b'DLMDSP01'+struct.pack('<I',count))
        for shape,line in cases:
            negative=line%3==1;bitstream.write(encode(shape,line,negative))
            for granule in range(2):
                target.write(bytes([[0,1,2,3,2,1,3][shape],2 if shape>=4 else 0,0,0]) if not granule else bytes(4))
                values=[0.0]*576
                if not granule:values[line]=-.5 if negative else .5
                target.write(struct.pack('<576d',*values))
    commands=[[str(args.native.resolve()),str(spectra.resolve()),str((args.output/'native.bin').resolve())],
              [str(args.oracle.resolve()),str(stream.resolve()),str((args.output/'reference.bin').resolve())]]
    for command in commands:subprocess.run(command,capture_output=True,check=True)
    # Fixed normalized f64-vs-libmad-Q28 limits, before measurements.
    limits={'hybrid':{'rms':2e-7,'max':1e-6},'pcm':{'rms':4e-7,'max':2e-6}}
    groups={};errors=[]
    with (args.output/'native.bin').open('rb') as native,(args.output/'reference.bin').open('rb') as reference:
        assert native.read(12)==reference.read(12)==b'DLMDOUT1'+struct.pack('<I',count)
        for shape,line in cases:
            for granule in range(2):
                for stage in ['hybrid','pcm']:
                    a=struct.unpack('<576d',native.read(576*8));b=struct.unpack('<576d',reference.read(576*8))
                    delta=[x-y for x,y in zip(a,b)];rms=math.sqrt(sum(v*v for v in delta)/576);peak=max(map(abs,delta))
                    group=groups.setdefault(f'{shape}/{stage}',{'samples':0,'max_rms':0,'max_error':0})
                    group['samples']+=576;group['max_rms']=max(group['max_rms'],rms);group['max_error']=max(group['max_error'],peak)
                    if rms>limits[stage]['rms'] or peak>limits[stage]['max']:errors.append({'shape':shape,'line':line,'granule':granule,'stage':stage,'rms':rms,'max':peak})
        assert not native.read(1) and not reference.read(1)
    sha=lambda path:hashlib.sha256(path.read_bytes()).hexdigest()
    report={'passed':not errors,'frames':len(cases),'granules':count,'groups':groups,'limits':limits,'errors':errors[:20],
            'method':__doc__,'commands':commands,'binaries':{str(path):sha(path) for path in [args.native,args.oracle]},
            'data_sha256':{p.name:sha(p) for p in args.output.iterdir() if p.is_file()}}
    (args.output/'results.json').write_text(json.dumps(report,indent=2)+'\n')
    print('frames',len(cases),'failed',len(errors),groups,flush=True);assert not errors,errors[:3]

if __name__=='__main__':main()
