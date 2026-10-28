"""Read-only real-file decoding against the retained four-reference fixture truth."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys


def sha(data):return hashlib.sha256(data).hexdigest()


def main():
    ap=argparse.ArgumentParser();ap.add_argument('--output',type=Path,required=True);args=ap.parse_args()
    if args.output.exists():ap.error('preserve prior evidence; choose a new output')
    root=Path(__file__).resolve().parents[1];workspace=root.parents[3]
    reference=root/'validation/adpcm-encoding-references.json';truth=json.loads(reference.read_text())
    sources=sorted((root/'src').rglob('*.rs'))+[root/'examples/adpcm_forward.rs',Path(__file__)]
    before={p.relative_to(root).as_posix():sha(p.read_bytes()) for p in sources}
    build=subprocess.run(['cargo','build','--locked','-p','deadlib-audio-decode-wav','--example','adpcm_forward','--target-dir','target/wav'],cwd=workspace,capture_output=True)
    args.output.parent.mkdir(parents=True,exist_ok=True);Path(str(args.output)+'.build.log').write_bytes(build.stdout+build.stderr)
    if build.returncode:raise RuntimeError('build failed; inspect preserved log')
    binary=workspace/('target/wav/debug/examples/adpcm_forward'+('.exe' if sys.platform=='win32' else ''))
    cases=[]
    for case in truth['cases']:
        path=workspace/'target/wav/encoded-adpcm'/f"{case['name']}.wav"
        if sha(path.read_bytes())!=case['fixture_sha256']:raise RuntimeError('retained corpus hash mismatch: '+str(path))
        for frames,width in ((1,1),(127,17),(4097,4096)):
            result=subprocess.run([str(binary),str(path),str(frames),str(width)],capture_output=True)
            output_sha=sha(result.stdout)
            cases.append(dict(name=case['name'],fixture_sha256=case['fixture_sha256'],buffer_frames=frames,read_width=width,exit_code=result.returncode,stderr=result.stderr.decode(errors='replace'),decoded_sha256=output_sha,passed=result.returncode==0 and output_sha==case['decoded_sha256']))
    after={p.relative_to(root).as_posix():sha(p.read_bytes()) for p in sources}
    report=dict(reference_sha256=sha(reference.read_bytes()),sources=before,inputs_unchanged=before==after,binary_sha256=sha(binary.read_bytes()),passed=sum(c['passed'] for c in cases),total=len(cases),cases=cases)
    args.output.write_text(json.dumps(report,indent=2)+'\n');print(f"{report['passed']}/{report['total']} Read-only ADPCM sequential cases passed")
    if report['passed']!=report['total'] or before!=after:raise SystemExit(1)


if __name__=='__main__':main()
