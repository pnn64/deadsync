"""Replay immutable native oracle hashes and exact writer outputs after optimization."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import subprocess
import sys
import time
import zipfile


def sha(data):return hashlib.sha256(data).hexdigest()


def main():
    ap=argparse.ArgumentParser();ap.add_argument('--output',type=Path,required=True);ap.add_argument('--bw64-legacy-writers',type=Path,help='verified prior-output archive; only replace BW64 dummy bytes 36..44 with zero');args=ap.parse_args()
    root=Path(__file__).resolve().parents[1];workspace=root.parents[3];output=args.output.resolve();output.parent.mkdir(parents=True,exist_ok=True)
    for suffix in ('','.build.log','-inputs'):
        if Path(str(output)+suffix).exists():ap.error('choose a new evidence destination')
    directory=Path(str(output)+'-inputs');directory.mkdir()
    paths=sorted((root/'src').rglob('*.rs'))+[root/'examples/pcm_reference.rs',Path(__file__),root/'Cargo.toml']
    before={p.relative_to(root).as_posix():sha(p.read_bytes()) for p in paths}
    prior={};prior_metadata=None
    if args.bw64_legacy_writers:
        sidecar=Path(str(args.bw64_legacy_writers)+'.json');metadata=json.loads(sidecar.read_text())
        assert sha(args.bw64_legacy_writers.read_bytes())==metadata['archive_sha256']
        with zipfile.ZipFile(args.bw64_legacy_writers) as archive:prior={name:archive.read(name) for name in archive.namelist()}
        assert {name:sha(data) for name,data in prior.items()}==metadata['files']
        prior_metadata=dict(archive_sha256=metadata['archive_sha256'],sidecar_sha256=sha(sidecar.read_bytes()),rule='only BW64 output bytes 36..44 replaced with zero; all other bytes unchanged')
    build=subprocess.run(['cargo','build','--locked','-p','deadlib-audio-decode-wav','--example','pcm_reference','--target-dir','target/wav'],cwd=workspace,capture_output=True);Path(str(output)+'.build.log').write_bytes(build.stdout+build.stderr)
    if build.returncode:raise RuntimeError('native driver build failed')
    binary=workspace/('target/wav/debug/examples/pcm_reference'+('.exe' if sys.platform=='win32' else ''))
    references=[];results=[]
    for bank,report_name,archive_name in [('flat','wavl-pcm-regression.json','pcm-canonical.corpus.zip'),('wavl','wavl-references.json','wavl-references.corpus.zip')]:
        report_path=root/'validation'/report_name;archive_path=root/'validation'/archive_name;reference=json.loads(report_path.read_text())
        assert reference['passed']==reference['total'] and reference['inputs_unchanged']
        assert sha(archive_path.read_bytes())==reference['corpus_sha256']
        references.append(dict(bank=bank,report=report_name,report_sha256=sha(report_path.read_bytes()),archive=archive_name,archive_sha256=reference['corpus_sha256']))
        with zipfile.ZipFile(archive_path) as archive:
            for case in reference['cases']:
                name=case['name'];payload=archive.read(name+'.wav');assert sha(payload)==case['fixture_sha256'];path=directory/(bank+'-'+name+'.wav');path.write_bytes(payload);checks={}
                for label,expected in case['ours'].items():
                    assert expected['exact']
                    command=[str(binary),str(path)]
                    if label=='describe':command+=['describe']
                    elif label.endswith('_range'):
                        mode=label.removesuffix('_range');start=case['frames']//2 if bank=='flat' or mode=='reader' else min(2,case['frames']);count=min(17,case['frames']-start);command += [mode,str(start),str(count),'5']
                    else:command += [label]
                    process=subprocess.run(command,capture_output=True)
                    if label=='describe':
                        try:actual=json.loads(process.stdout)
                        except Exception:actual={}
                        actual['container']=actual.get('container','').upper();passed=process.returncode==0 and actual==expected['expected']
                        row=dict(passed=passed,exit_code=process.returncode,actual=actual,expected=expected['expected'])
                    else:row=dict(passed=process.returncode==0 and sha(process.stdout)==expected['output_sha256'],exit_code=process.returncode,output_sha256=sha(process.stdout),expected_sha256=expected['output_sha256'],output_bytes=len(process.stdout))
                    row['stderr']=process.stderr.decode(errors='replace')[:1500];checks[label]=row
                encoded={}
                for mode,expected in case['encoded'].items():
                    assert expected['exact'] and expected['encoder_exit_code']==0
                    expected_sha=expected['encoded_sha256'];adapted=False
                    if prior and payload[:4]==b'BW64':
                        data=prior[bank+'-'+name+'-'+mode+'.wav'];assert sha(data)==expected_sha
                        assert data[:4]==b'BW64' and data[12:16]==b'ds64'
                        expected_sha=sha(data[:36]+b'\0'*8+data[44:]);adapted=True
                    process=subprocess.run([str(binary),str(path),mode],capture_output=True)
                    encoded[mode]=dict(passed=process.returncode==0 and sha(process.stdout)==expected_sha,exit_code=process.returncode,encoded_sha256=sha(process.stdout),expected_sha256=expected_sha,prior_sha256=expected['encoded_sha256'],bw64_dummy_rule=adapted,encoded_bytes=len(process.stdout),stderr=process.stderr.decode(errors='replace')[:1500])
                passed=all(c['passed'] for c in checks.values()) and all(c['passed'] for c in encoded.values())
                results.append(dict(bank=bank,name=name,fixture_sha256=case['fixture_sha256'],checks=checks,encoded=encoded,passed=passed))
                if len(results)%80==0:print(f'{len(results)} retained native cases; failures={sum(not c["passed"] for c in results)}',flush=True)
    after={p.relative_to(root).as_posix():sha(p.read_bytes()) for p in paths}
    report=dict(timestamp_utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),platform=platform.platform(),rustc=subprocess.check_output(['rustc','-Vv'],text=True),sources=before,inputs_unchanged=before==after,binary_sha256=sha(binary.read_bytes()),references=references,bw64_prior_outputs=prior_metadata,passed=sum(c['passed'] for c in results),total=len(results),decode_descriptor_checks=sum(len(c['checks']) for c in results),writer_checks=sum(len(c['encoded']) for c in results),cases=results)
    output.write_text(json.dumps(report,indent=2)+'\n');print(f"{report['passed']}/{report['total']} retained native gates; {report['decode_descriptor_checks']} decode/descriptor/range checks; {report['writer_checks']} exact encoded-output checks",flush=True)
    if report['passed']!=report['total'] or before!=after:raise SystemExit(1)


if __name__=='__main__':main()
