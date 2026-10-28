"""Retain prior BW64 writer bytes after checking immutable reference hashes."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import zipfile


def sha(data):return hashlib.sha256(data).hexdigest()


def main():
    ap=argparse.ArgumentParser();ap.add_argument('--binary',type=Path,required=True);ap.add_argument('--output',type=Path,required=True);args=ap.parse_args()
    root=Path(__file__).resolve().parents[1];output=args.output.resolve();sidecar=Path(str(output)+'.json')
    if output.exists() or sidecar.exists():ap.error('choose a new archive destination')
    baseline=root/'validation/rf64-count-native-replay.json';baseline_report=json.loads(baseline.read_text())
    assert sha(args.binary.read_bytes())==baseline_report['binary_sha256']
    paths={};references=[];directory=output.parent/(output.name+'-inputs');directory.mkdir(parents=True,exist_ok=False)
    for bank,report_name,archive_name in [('flat','wavl-pcm-regression.json','pcm-canonical.corpus.zip'),('wavl','wavl-references.json','wavl-references.corpus.zip')]:
        report_path=root/'validation'/report_name;archive_path=root/'validation'/archive_name;report=json.loads(report_path.read_text())
        assert sha(archive_path.read_bytes())==report['corpus_sha256']
        references.append(dict(bank=bank,report=report_name,report_sha256=sha(report_path.read_bytes()),archive=archive_name,archive_sha256=report['corpus_sha256']))
        with zipfile.ZipFile(archive_path) as archive:
            for case in report['cases']:
                fixture=archive.read(case['name']+'.wav');assert sha(fixture)==case['fixture_sha256']
                if fixture[:4]!=b'BW64':continue
                path=directory/(bank+'-'+case['name']+'.wav');path.write_bytes(fixture)
                for mode,expected in case['encoded'].items():
                    process=subprocess.run([str(args.binary.resolve()),str(path),mode],capture_output=True)
                    assert process.returncode==0 and sha(process.stdout)==expected['encoded_sha256']
                    assert process.stdout[:4]==b'BW64' and process.stdout[12:16]==b'ds64'
                    paths[bank+'-'+case['name']+'-'+mode+'.wav']=process.stdout
    output.parent.mkdir(parents=True,exist_ok=True)
    with zipfile.ZipFile(output,'x',compression=zipfile.ZIP_DEFLATED,compresslevel=9) as archive:
        for name,data in sorted(paths.items()):
            info=zipfile.ZipInfo(name,date_time=(1980,1,1,0,0,0));info.create_system=3;info.external_attr=0o600<<16
            archive.writestr(info,data,compress_type=zipfile.ZIP_DEFLATED,compresslevel=9)
    manifest={name:sha(data) for name,data in sorted(paths.items())}
    with zipfile.ZipFile(output) as archive:assert {name:sha(archive.read(name)) for name in archive.namelist()}==manifest
    sidecar.write_text(json.dumps(dict(archive_sha256=sha(output.read_bytes()),entries=len(paths),expanded_bytes=sum(map(len,paths.values())),baseline_report_sha256=sha(baseline.read_bytes()),baseline_binary_sha256=baseline_report['binary_sha256'],references=references,tool_sha256=sha(Path(__file__).read_bytes()),files=manifest),indent=2)+'\n')
    print(f'{len(paths)} prior BW64 writer outputs match immutable expected hashes')


if __name__=='__main__':main()
