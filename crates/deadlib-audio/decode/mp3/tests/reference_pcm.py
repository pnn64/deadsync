"""Raw framed PCM against public libmad/mpg123 and Symphonia decoders.

Includes Info/Xing-frame samples and synthesis delay. File-level gapless trimming
is intentionally a separate gate. Every raw frame's rate/channel/count is exact.
"""
import argparse,hashlib,json,math,struct,subprocess
from pathlib import Path

def frames(path):
    result=[]
    with path.open('rb') as data:
        assert data.read(8)==b'DLMP3PC1'
        while header:=data.read(12):
            rate,channels,samples=struct.unpack('<III',header)
            assert channels in [1,2] and samples in [576,1152]
            pcm=struct.unpack('<'+'f'*(samples*channels),data.read(samples*channels*4))
            result.append(((rate,channels,samples),pcm))
    return result

def main():
    p=argparse.ArgumentParser()
    for name in ['native','oracle','corpus','output']:p.add_argument('--'+name,type=Path,required=True)
    p.add_argument('--case',action='append')
    p.add_argument('--allow-symphonia-free',action='store_true',help='record Symphonia 0.6.1 free-bitrate rejection; still require both C references')
    p.add_argument('--survey',action='store_true',help='investigate reference disagreements; report failures without a passing conformance claim')
    args=p.parse_args();args.output.mkdir(parents=True,exist_ok=False)
    # Normalized unclipped f32, with fixed-point libmad intermediary rounding.
    limits={'rms':2e-6,'max':2e-5}
    records=[]
    for source in sorted(args.corpus.glob('*.mp3')):
        if args.case and not any(source.name.startswith(prefix) for prefix in args.case):continue
        outputs={};commands=[];unsupported=[]
        for mode in ['native','mad','mpg123','symphonia']:
            target=args.output/(source.stem+'-'+mode+'.bin')
            command=[str(args.native.resolve() if mode in ['native','symphonia'] else args.oracle.resolve()),mode,str(source.resolve()),str(target.resolve())]
            run=subprocess.run(command,capture_output=True,text=True)
            commands.append({'command':command,'exit_code':run.returncode,'stderr':run.stderr})
            if run.returncode:
                if args.allow_symphonia_free and mode=='symphonia' and '-free-' in source.name and run.returncode==1 and 'mpa: free bit-rate is not supported' in run.stderr:
                    unsupported.append({'decoder':mode,'reason':run.stderr.strip()});continue
                (args.output/'failure.json').write_text(json.dumps({'source':str(source),'commands':commands},indent=2)+'\n')
                raise RuntimeError(commands[-1])
            outputs[mode]=frames(target)
        native=outputs['native'];comparisons={}
        for mode in ['mad','mpg123','symphonia']:
            if mode not in outputs:continue
            reference=outputs[mode];assert len(native)==len(reference),(source.name,mode,'frame count',len(native),len(reference))
            total=0;energy=0;peak=0
            for (a,x),(b,y) in zip(native,reference):
                assert a==b,(source.name,mode,a,b)
                for left,right in zip(x,y):
                    assert math.isfinite(left) and math.isfinite(right)
                    difference=abs(left-right);energy+=difference*difference;peak=max(peak,difference);total+=1
            comparisons[mode]={'samples':total,'rms':math.sqrt(energy/max(1,total)),'max':peak}
        record={'case':source.name,'sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'frames':len(native),'comparisons':comparisons,'commands':commands,'unsupported':unsupported}
        records.append(record);(args.output/'results.json').write_text(json.dumps({'limits':limits,'records':records},indent=2)+'\n')
        print(source.name,{m:round(c['max'],9) for m,c in comparisons.items()},flush=True)
        if not args.survey:assert all(c['rms']<=limits['rms'] and c['max']<=limits['max'] for c in comparisons.values()),record
    sha=lambda path:hashlib.sha256(path.read_bytes()).hexdigest()
    passed=bool(records) and all(c['rms']<=limits['rms'] and c['max']<=limits['max'] for r in records for c in r['comparisons'].values())
    (args.output/'provenance.json').write_text(json.dumps({'passed':passed,'survey':args.survey,'files':len(records),'method':__doc__,'binaries':{str(path):sha(path) for path in [args.native,args.oracle]},'reference_versions':subprocess.check_output(['pkg-config','--modversion','mad','libmpg123'],text=True).splitlines()},indent=2)+'\n')

if __name__=='__main__':main()
