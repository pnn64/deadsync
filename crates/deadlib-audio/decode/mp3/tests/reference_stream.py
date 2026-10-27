"""Streaming byte-for-byte PCM and exact positions against the raw-frame gate.

Read sizes include direct File IO and one-byte through multi-frame reads. The
expected raw PCM is independently gated against libmad/mpg123/Symphonia; this
gate isolates buffering, state, metadata skipping and sample/byte coordinates.
"""
import argparse, hashlib, json, subprocess
from pathlib import Path
from reference_pcm import frames

p=argparse.ArgumentParser()
for name in ['native','corpus','reference','output']:p.add_argument('--'+name,type=Path,required=True)
args=p.parse_args();args.output.mkdir(parents=True,exist_ok=False)
framing_path=Path(__file__).parent/'validation/2026-10-27-foundation/reference-framing.json'
framing={Path(r['command'][-1]).name:r for r in json.loads(framing_path.read_text())}
records=[];chunks=[0,1,2,3,7,31,127,1024,8192]
for source in sorted(args.corpus.glob('*.mp3')):
    proof=framing[source.name]
    assert hashlib.sha256(source.read_bytes()).hexdigest()==proof['sha256']
    expected_offsets=[int(line.split('\t')[1]) for line in proof['libmad']['stdout'].splitlines() if line.startswith('frame\t')]
    expected=args.reference/(source.stem+'-native.bin');expected_bytes=expected.read_bytes();expected_frames=frames(expected)
    runs=[]
    for chunk in chunks:
        output=args.output/(source.stem+'-'+str(chunk)+'.bin')
        command=[str(args.native.resolve()),str(source.resolve()),str(output.resolve()),str(chunk)]
        run=subprocess.run(command,capture_output=True,text=True)
        record={'chunk':chunk,'command':command,'exit_code':run.returncode,'stdout':run.stdout,'stderr':run.stderr}
        runs.append(record)
        assert run.returncode==0,record
        assert output.read_bytes()==expected_bytes,(source.name,chunk,'PCM differs')
        rows=[line.split('\t')[1:] for line in run.stdout.splitlines() if line.startswith('frame\t')]
        assert len(rows)==len(expected_frames)
        sample=0;offsets=[]
        for row,(header,_) in zip(rows,expected_frames):
            offset,position,samples,status=row
            assert int(position)==sample and int(samples)==header[2] and status=='Decoded',(source.name,chunk,row)
            offsets.append(int(offset));sample+=header[2]
        assert offsets==expected_offsets,(source.name,chunk,'libmad byte offsets differ')
        summary=next(line.split('\t')[1:] for line in run.stdout.splitlines() if line.startswith('summary\t'))
        assert list(map(int,summary[:5]))==[len(expected_frames),0,sample,0,0],summary
        expected_tag=next((line for line in proof['native']['stdout'].splitlines() if line.startswith('xing\t')),None)
        tag=next((line for line in run.stdout.splitlines() if line.startswith('xing\t')),None)
        assert tag==expected_tag,(source.name,chunk,'retained Xing/Info metadata differs',tag,expected_tag)
        record['pcm_sha256']=hashlib.sha256(expected_bytes).hexdigest()
        output.unlink() # No need to retain duplicate PCM for nine read schedules.
    records.append({'file':source.name,'input_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'frames':len(expected_frames),'runs':runs})
    (args.output/'results.json').write_text(json.dumps({'passed':True,'method':__doc__,'chunks':chunks,'records':records},indent=2)+'\n')
    print(source.name,'all',len(chunks),'read schedules pass',flush=True)
report={'passed':bool(records),'files':len(records),'read_schedules':len(records)*len(chunks),'binary_sha256':hashlib.sha256(args.native.read_bytes()).hexdigest()}
(args.output/'provenance.json').write_text(json.dumps(report,indent=2)+'\n')
assert report['passed']
