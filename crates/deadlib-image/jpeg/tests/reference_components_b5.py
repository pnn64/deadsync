"""Replay ordinary pinned streams through IJG, plus real B.5 255-component pairs."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
parser=argparse.ArgumentParser()
for name in ('author','ijg','fixtures','out'):parser.add_argument('--'+name,required=True,type=Path)
args=parser.parse_args();args.out.mkdir(parents=True,exist_ok=True)
tools=args.out/'tools';tools.mkdir(exist_ok=True);hashes={}
for name in ('author','ijg'):
    source=getattr(args,name);target=tools/(name+source.suffix);shutil.copy2(source,target)
    hashes[name]=hashlib.sha256(target.read_bytes()).hexdigest();setattr(args,name,target.resolve())
def run(command):
    r=subprocess.run([str(s)for s in command],capture_output=True,timeout=60)
    assert not r.returncode and not r.stderr,(command,r.returncode,r.stderr)
def sha(data):return hashlib.sha256(data).hexdigest()
manifest=json.loads((args.fixtures/'manifest.json').read_text())
manifest['sessions']=[r for r in manifest['sessions']if not r['case'].startswith('b5-255-')]
rows=[]
for row in manifest['sessions']:
    if any(e['file'].endswith('.samples')for e in row['expected']):continue
    expected=next(e for e in row['expected']if e['file'].endswith('.coef'))
    streams=[args.fixtures/e['file']for e in row['streams']]
    run([args.ijg,*streams,args.out/'observed.coef'])
    assert(args.out/'observed.coef').read_bytes()==(args.fixtures/expected['file']).read_bytes()
    rows.append({'case':row['case'],'genuine_B5':len(streams)==2,'coefficient_sha256':expected['sha256']})
for p in (8,12):
    for arithmetic in (0,1):
        prefix=args.out/'large'
        run([args.author,prefix,255,p,1,2 if p==8 else 3,arithmetic,0,3,0,0,0,1,0])
        table=args.out/'large.tables.jpg';image=args.out/'large.jpg';expected=args.out/'large.expected.frame-0.coef'
        run([args.ijg,table,image,args.out/'observed.coef'])
        assert(args.out/'observed.coef').read_bytes()==expected.read_bytes()
        name=f'b5-255-p{p}-a{arithmetic}';streams=[]
        for i,path in enumerate((table,image)):
            dest=args.fixtures/f'{name}.stream-{i}.jpg';dest.write_bytes(path.read_bytes())
            streams.append({'file':dest.name,'sha256':sha(dest.read_bytes())})
        dest=args.fixtures/f'{name}.frame-0.coef';dest.write_bytes(expected.read_bytes())
        manifest['sessions'].append({'case':name,'part':'native','streams':streams,
            'expected':[{'file':dest.name,'sha256':sha(dest.read_bytes())}],
            'new_coverage':[f'genuine IJG B.5; Nf=255 P={p} arithmetic={arithmetic}; fractional sampling']})
        rows.append({'case':name,'genuine_B5':True,'coefficient_sha256':sha(expected.read_bytes())})
(args.fixtures/'manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
report={'scope':__doc__,'tools':hashes,'driver_sha256':sha(Path(__file__).read_bytes()),'cases':len(rows),
        'genuine_B5_cases':sum(r['genuine_B5']for r in rows),'results':rows}
(args.out/'report.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items()if k!='results'}))
