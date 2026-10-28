"""Preserve exact corpus bytes in a deterministic ZIP after checking run hashes."""
import argparse
import hashlib
import json
from pathlib import Path
import zipfile


def sha(data):return hashlib.sha256(data).hexdigest()


def main():
    ap=argparse.ArgumentParser();ap.add_argument('--report',type=Path,required=True);ap.add_argument('--corpus',type=Path,required=True);ap.add_argument('--output',type=Path,required=True);args=ap.parse_args()
    if args.output.exists() or Path(str(args.output)+'.json').exists():ap.error('choose a new output archive')
    report=json.loads(args.report.read_text());expected=report['final_corpus']
    actual={}
    for path in sorted(args.corpus.iterdir()):
        if path.is_file():
            if path.name!=str(Path(path.name)) or ':' in path.name or '\\' in path.name or '/' in path.name:ap.error('corpus must have flat portable names')
            data=path.read_bytes()
            if len(data)>32768:ap.error('corpus input exceeds harness budget')
            actual[path.name]=data
    if {name:sha(data) for name,data in actual.items()}!=expected:ap.error('corpus bytes do not match the completed run manifest')
    args.output.parent.mkdir(parents=True,exist_ok=True)
    with zipfile.ZipFile(args.output,'x',compression=zipfile.ZIP_DEFLATED,compresslevel=9) as archive:
        for name,data in actual.items():
            info=zipfile.ZipInfo(name,date_time=(1980,1,1,0,0,0));info.create_system=3;info.external_attr=0o600<<16
            archive.writestr(info,data,compress_type=zipfile.ZIP_DEFLATED,compresslevel=9)
    with zipfile.ZipFile(args.output) as archive:
        assert {name:sha(archive.read(name)) for name in archive.namelist()}==expected
    metadata=dict(archive_sha256=sha(args.output.read_bytes()),archive_bytes=args.output.stat().st_size,entries=len(actual),expanded_bytes=sum(map(len,actual.values())),source_report_sha256=sha(args.report.read_bytes()))
    Path(str(args.output)+'.json').write_text(json.dumps(metadata,indent=2)+'\n');print(json.dumps(metadata))


if __name__=='__main__':main()
