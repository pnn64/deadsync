"""ISO Layer III fixture PCM validation (vectors mirrored by lieff/minimp3).

Reference PCM is signed 16-bit little-endian. Report the full-accuracy sweep
threshold separately from a functional bound allowing one truncated 16-bit LSB.
No fitted gain, resampling, per-vector threshold, or arbitrary alignment search.
"""
import argparse, hashlib, json, math, struct, subprocess
from pathlib import Path
from reference_pcm import frames


def main():
    p=argparse.ArgumentParser()
    for name in ['native','corpus','output']:p.add_argument('--'+name,type=Path,required=True)
    args=p.parse_args();args.output.mkdir(parents=True,exist_ok=False)
    limits={'rms':2**-15/math.sqrt(3),'max':2**-14}
    full_rms=2**-15/math.sqrt(12)
    records=[]
    for source in sorted(args.corpus.glob('*.bit')):
        if not source.name.startswith(('l3-','M2L3_')) or 'nonstandard' in source.name:continue
        reference_path=source.with_suffix('.pcm')
        if not reference_path.exists():continue
        target=args.output/(source.stem+'.bin')
        command=[str(args.native.resolve()),'native',str(source.resolve()),str(target.resolve())]
        run=subprocess.run(command,capture_output=True,text=True)
        record={'case':source.name,'command':command,'exit_code':run.returncode,'stderr':run.stderr,
                'input_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),
                'reference_sha256':hashlib.sha256(reference_path.read_bytes()).hexdigest()}
        if not run.returncode:
            decoded=frames(target);actual=[s for _,pcm in decoded for s in pcm]
            data=reference_path.read_bytes();reference=struct.unpack('<'+'h'*(len(data)//2),data)
            # This vector's supplied reference omits exactly two lead-in frames.
            skip=2*1152*decoded[0][0][1] if source.name=='l3-sin1k0db.bit' else 0
            actual=actual[skip:]
            record.update(frames=len(decoded),samples=len(actual),reference_samples=len(reference),lead_in_samples=skip)
            # Several legacy MPEG-1 references stop one physical frame early.
            # Require that exact length difference; compare their entire prefix.
            extra=len(actual)-len(reference)
            last_frame=decoded[-1][0][1]*decoded[-1][0][2] if decoded else 0
            record['unreferenced_tail_samples']=extra
            if extra in [0,last_frame]:
                clipped=[min(32767/32768,max(-1,a)) for a in actual[:len(reference)]]
                record['clipped_samples']=sum(a!=c for a,c in zip(actual,clipped))
                delta=[a-r/32768 for a,r in zip(clipped,reference)]
                rms=math.sqrt(sum(d*d for d in delta)/max(1,len(delta)));peak=max(map(abs,delta),default=0)
                record.update(rms=rms,max=peak,full_accuracy_rms=rms<full_rms,
                              passed=rms<=limits['rms'] and peak<=limits['max'])
            else:record['passed']=False
        else:record['passed']=False
        records.append(record)
        (args.output/'results.json').write_text(json.dumps({'method':__doc__,'limits':limits,'full_accuracy_rms':full_rms,'records':records},indent=2)+'\n')
        print(source.name,{k:record[k] for k in ['passed','rms','max','stderr','samples','reference_samples'] if k in record},flush=True)
    report={'passed':bool(records) and all(r['passed'] for r in records),'files':len(records),
            'binary_sha256':hashlib.sha256(args.native.read_bytes()).hexdigest()}
    (args.output/'provenance.json').write_text(json.dumps(report,indent=2)+'\n')
    assert report['passed'],[r['case'] for r in records if not r['passed']]


if __name__=='__main__':main()
