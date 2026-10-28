"""Real-file sequential/range checks against the retained four-reference corpus."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys


def sha(b):
    return hashlib.sha256(b).hexdigest()


def main():
    ap=argparse.ArgumentParser()
    ap.add_argument('--output',type=Path,required=True)
    ap.add_argument('--reference',type=Path)
    args=ap.parse_args()
    root=Path(__file__).resolve().parents[1]
    workspace=root.parents[3]
    reference=args.reference or root/'validation/adpcm-encoding-references.json'
    report=json.loads(reference.read_text())
    subprocess.run(['cargo','build','--locked','-p','deadlib-audio-decode-wav','--examples','--target-dir','target/wav'],cwd=workspace,check=True,capture_output=True)
    suffix='.exe' if sys.platform=='win32' else ''
    binary=workspace/f'target/wav/debug/examples/adpcm_file{suffix}'
    borrowed=workspace/f'target/wav/debug/examples/adpcm{suffix}'
    results=[]
    for case in report['cases']:
        path=workspace/'target/wav/encoded-adpcm'/f"{case['name']}.wav"
        if sha(path.read_bytes())!=case['fixture_sha256']:
            raise RuntimeError('corpus differs from retained reference validation: '+str(path))
        process=subprocess.run([str(binary),str(path)],capture_output=True,check=True)
        results.append(dict(name=case['name'],start=0,frames=None,output_sha256=sha(process.stdout),passed=sha(process.stdout)==case['decoded_sha256']))
        parts=case['name'].split('-')
        if parts[-2:]!=['1025','noise']:
            continue
        channels=int(parts[1]);block=int(parts[2]);frames=case['source_samples']//channels
        block_frames=(block//channels-(4 if parts[0]=='ima' else 7))*2+(1 if parts[0]=='ima' else 2)
        expected=subprocess.run([str(borrowed),str(path)],capture_output=True,check=True).stdout
        if sha(expected)!=case['decoded_sha256']:
            raise RuntimeError('borrowed decoder differs from reference campaign')
        for start,count in [(0,1),(1,17),(block_frames-1,17),(frames-17,17),(frames,0)]:
            process=subprocess.run([str(binary),str(path),str(start),str(count)],capture_output=True,check=True)
            wanted=expected[start*channels*2:(start+count)*channels*2]
            results.append(dict(name=case['name'],start=start,frames=count,output_sha256=sha(process.stdout),passed=process.stdout==wanted))
    sources=sorted((root/'src').glob('*.rs'))+[root/'examples/adpcm_file.rs',Path(__file__)]
    evidence=dict(reference_sha256=sha(reference.read_bytes()),binary_sha256=sha(binary.read_bytes()),
                  sources={p.relative_to(root).as_posix():sha(p.read_bytes()) for p in sources},
                  passed=sum(c['passed'] for c in results),total=len(results),cases=results)
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(evidence,indent=2)+'\n')
    print(f"{evidence['passed']}/{evidence['total']} real-file ADPCM sequential/range cases passed")
    if evidence['passed']!=evidence['total']:
        raise SystemExit(1)


if __name__=='__main__':
    main()
