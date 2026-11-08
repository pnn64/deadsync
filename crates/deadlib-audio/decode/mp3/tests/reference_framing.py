"""Generate LAME fixtures and compare native framing with libmad, using Debian.

Run on Linux/WSL with native Rust `inspect` and the compiled C oracle. The fixtures
and reports live outside tracked source; PCM decoding parity is a later gate.
"""
import argparse,hashlib,json,math,random,struct,subprocess,wave
from pathlib import Path

def main():
    parser=argparse.ArgumentParser()
    for name in ['native','oracle','output']:parser.add_argument('--'+name,type=Path,required=True)
    parser.add_argument('--scalefactors',action='store_true')
    args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=False)
    records=[];rng=random.Random(9639)
    for rate in [8000,11025,12000,16000,22050,24000,32000,44100,48000]:
        for channels in [1,2]:
            pcm=bytearray()
            for n in range(4096):
                for c in range(channels):
                    sample=int(9000*math.sin(n*(0.11+c*0.023))+rng.uniform(-1200,1200))
                    pcm+=struct.pack('<h',sample)
            source=args.output/f'{rate}-{channels}.wav'
            with wave.open(str(source),'wb') as out:out.setparams((channels,2,rate,4096,'NONE',''));out.writeframes(pcm)
            for mode in ['cbr','vbr']+(['free'] if rate>=32000 else []):
                for crc in [False,True]:
                    target=source.with_name(f'{rate}-{channels}-{mode}-{int(crc)}.mp3')
                    options=['-V','4'] if mode=='vbr' else ['-b','128' if rate>=32000 else '32']
                    if mode=='free':options=['--freeformat','-b','400']
                    command=['lame','--silent','--noreplaygain','--resample',str(rate/1000),'-m','m' if channels==1 else 'j']+options+(['-p'] if crc else [])+[str(source),str(target)]
                    subprocess.run(command,capture_output=True,check=True)
                    a=subprocess.run([str(args.native.resolve()),str(target)]+(['--scalefactors'] if args.scalefactors else []),capture_output=True,text=True)
                    b=subprocess.run([str(args.oracle.resolve()),str(target)],capture_output=True,text=True)
                    record={'rate':rate,'channels':channels,'mode':mode,'crc':crc,'command':command,'sha256':hashlib.sha256(target.read_bytes()).hexdigest(),'native':{'exit_code':a.returncode,'stdout':a.stdout,'stderr':a.stderr},'libmad':{'exit_code':b.returncode,'stdout':b.stdout,'stderr':b.stderr}}
                    records.append(record)
                    (args.output/'results.json').write_text(json.dumps(records,indent=2)+'\n')
                    assert not a.returncode and not b.returncode,record
                    rows=lambda text:[list(map(int,line.split('\t')[1:])) for line in text.splitlines() if line.startswith('frame\t')]
                    native,reference=rows(a.stdout),rows(b.stdout)
                    assert len(native)==len(reference),record
                    for left,right in zip(native,reference):
                        assert left[0]==right[0] and left[2:]==right[2:],record
                        assert left[1]==right[1] or mode=='free',record
                        assert left[2:4]==[rate,channels],record
                    tag=next((list(map(int,line.split('\t')[1:])) for line in a.stdout.splitlines() if line.startswith('xing\t')),None)
                    if tag:
                        frames,delay,padding=tag
                        assert frames==len(native)-1,record
                        assert frames*native[-1][4]-delay-padding==4096,record
                    print(target.name,len(native),'frames',flush=True)
    versions={name:subprocess.check_output(command,text=True,stderr=subprocess.STDOUT).strip() for name,command in [('lame',['lame','--version']),('references',['pkg-config','--modversion','mad','libmpg123','lame'])]}
    (args.output/'provenance.json').write_text(json.dumps({'versions':versions,'binaries':{str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in [args.native,args.oracle]},'fixtures':len(records),'scalefactors':args.scalefactors,'scope':'frame offsets/counts/rates/channels/sizes, side-info/reservoir acceptance, CRC, LAME raw delay/padding accounting. Optional part2 syntax/length validation. No native PCM decoding.'},indent=2)+'\n')

if __name__=='__main__':main()
