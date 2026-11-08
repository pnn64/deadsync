"""Deterministic multi-second, non-frame-aligned LAME files for gapless seeking."""
import argparse,hashlib,json,math,random,struct,subprocess,wave
from pathlib import Path

p=argparse.ArgumentParser();p.add_argument('--output',type=Path,required=True)
args=p.parse_args();args.output.mkdir(parents=True,exist_ok=False);records={}
for rate in [8000,11025,12000,16000,22050,24000,32000,44100,48000]:
    for channels in [1,2]:
        samples=3*rate+137;rng=random.Random(0x9381+rate+channels);data=bytearray()
        for n in range(samples):
            t=n/rate;burst=1 if (n%(rate//4))<rate//50 else .05
            quiet=.05 if .8<t<1.0 else 1
            for c in range(channels):
                value=quiet*(8000*math.sin(2*math.pi*(211+43*c)*t)+4000*math.sin(2*math.pi*(53*t+997*t*t))+burst*rng.uniform(-5000,5000))
                data.extend(struct.pack('<h',int(value)))
        source=args.output/f'{rate}-{channels}.wav'
        with wave.open(str(source),'wb') as out:out.setparams((channels,2,rate,samples,'NONE',''));out.writeframes(data)
        for mode in ['cbr','vbr']:
            target=source.with_name(f'{rate}-{channels}-{mode}.mp3')
            bitrate=128 if rate>=32000 else 64 if rate>=16000 else 32
            cmd=['lame','--silent','--noreplaygain','--resample',str(rate/1000),'-m','m' if channels==1 else 'j']+(['-V','3'] if mode=='vbr' else ['-b',str(bitrate)])+[str(source.resolve()),str(target.resolve())]
            subprocess.run(cmd,capture_output=True,check=True)
            records[target.name]={'samples':samples,'rate':rate,'channels':channels,'command':cmd,'source_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'input_sha256':hashlib.sha256(target.read_bytes()).hexdigest()}
            (args.output/'sources.json').write_text(json.dumps(records,indent=2)+'\n')
            print(target.name,samples,flush=True)
