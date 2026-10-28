"""Repeat immutable before/after native benchmark binaries on identical PCM32 data."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def sha(b):return hashlib.sha256(b).hexdigest()


def main():
    ap=argparse.ArgumentParser();ap.add_argument('--before',type=Path,required=True);ap.add_argument('--after',type=Path,required=True);ap.add_argument('--output',type=Path,required=True);args=ap.parse_args()
    if args.output.exists() or Path(str(args.output)+'-raw').exists():ap.error('preserve prior evidence; choose a new output')
    reports=[json.loads(p.read_text()) for p in (args.before,args.after)];binaries=[]
    for path,r in zip((args.before,args.after),reports):
        directory=Path(str(path.with_suffix(''))+'-bin');files=[p for p in directory.iterdir() if p.is_file()];assert len(files)==1
        assert sha(files[0].read_bytes())==r['binary_sha256'];binaries.append(files[0].resolve())
    before={r['case']['name']:r for r in reports[0]['runs']};cases=[r for r in reports[1]['runs'] if r['case']['kind']=='i32' and r['case']['channels']==2 and r['case']['valid_bits']==32 and r['case']['metadata_chunks']==0]
    assert len(cases)==5
    raw=Path(str(args.output)+'-raw');raw.mkdir(parents=True);runs=[]
    for repeat in range(2):
        for case in cases:
            info=case['case'];assert info['fixture_sha256']==before[info['name']]['case']['fixture_sha256'];assert sha(Path(case['command'][2]).read_bytes())==info['fixture_sha256']
            for label,binary in zip(('before','after'),binaries):
                command=[str(binary)]+case['command'][1:];p=subprocess.run(command,capture_output=True);name=f'{repeat}-{info["name"]}-{label}'
                (raw/(name+'.tsv')).write_bytes(p.stdout);(raw/(name+'.stderr')).write_bytes(p.stderr)
                runs.append(dict(repeat=repeat,case=info['name'],version=label,command=command,exit_code=p.returncode,stdout_sha256=sha(p.stdout),stderr_sha256=sha(p.stderr)))
                assert p.returncode==0,p.stderr.decode(errors='replace')
        print(f'paired repetition {repeat+1} passed',flush=True)
    args.output.write_text(json.dumps(dict(before_report_sha256=sha(args.before.read_bytes()),after_report_sha256=sha(args.after.read_bytes()),tool_sha256=sha(Path(__file__).read_bytes()),source_changes={k:[reports[0]['sources'].get(k),v] for k,v in reports[1]['sources'].items() if reports[0]['sources'].get(k)!=v},binary_sha256=[r['binary_sha256'] for r in reports],runs=runs,scope='immutable private binaries from completed source-fingerprinted runs; two alternating paired repetitions; identical fixture bytes; three warmed internal trials; no affinity/frequency control'),indent=2)+'\n')


if __name__=='__main__':main()
