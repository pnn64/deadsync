"""Retain representative checked streams without duplicating large 255-plane cases."""
import argparse
import hashlib
import json
from pathlib import Path
parser=argparse.ArgumentParser()
for name in ('raw','out','native_report'):parser.add_argument('--'+name.replace('_','-'),required=True,type=Path)
args=parser.parse_args();assert args.raw.resolve()!=args.out.resolve()
args.out.mkdir(parents=True,exist_ok=True)
params={'native-'+r['case']:r['params']for r in json.loads(args.native_report.read_text())['results']}
sessions=[]
for part in ('c','native'):
    manifest=json.loads((args.raw/(part+'-manifest.json')).read_text())
    for row in manifest['sessions']:
        if part=='native':
            n,p,_,q,arith,_,_,_,mode,_,_=params[row['case']]
            if n==255 and not(q==(2 if p==8 else 3)and arith==(mode%2 if p==8 else 1-mode%2)):
                continue
        for entry in row['streams']+row['expected']:
            data=(args.raw/entry['file']).read_bytes()
            assert hashlib.sha256(data).hexdigest()==entry['sha256']
            (args.out/entry['file']).write_bytes(data)
        sessions.append(row)
result={'scope':__doc__,'provenance':'Native expectations captured before entropy encoding. C-authored coefficient files match IJG/JPEG XT/native decoding. These are regressions, not an independent conformance corpus.',
        'driver_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'sessions':sessions}
(args.out/'manifest.json').write_text(json.dumps(result,indent=2)+'\n')
print(json.dumps({'sessions':len(sessions),'files':len(list(args.out.iterdir())),
                  'bytes':sum(p.stat().st_size for p in args.out.iterdir())}))
