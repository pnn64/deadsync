"""Verify the added generic-component observer leaves earlier frame logs intact."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
from hierarchy_support import structure
from stream_support import clear_stale_exp
parser=argparse.ArgumentParser()
for name in ('before','after','out'):parser.add_argument('--'+name,required=True,type=Path)
args=parser.parse_args();args.out.mkdir(parents=True,exist_ok=True)
tools=args.out/'tools';tools.mkdir(exist_ok=True);hashes={}
for name in ('before','after'):
    source=getattr(args,name);target=tools/(name+source.suffix);shutil.copy2(source,target)
    hashes[name]=hashlib.sha256(target.read_bytes()).hexdigest();setattr(args,name,target.resolve())
root=Path(__file__).parent/'fixtures';rows=[]
for directory in ('sequential','progressive','arithmetic','arithmetic-progressive','arithmetic-model','hierarchical','hierarchical-native','pyramid-native'):
    for source in sorted((root/directory).glob('*.jpg')):
        wire=source.read_bytes();_,frames,_,_,_=structure(wire,False)
        fixed=clear_stale_exp(wire,frames);image=args.out/'image.jpg';image.write_bytes(fixed)
        outputs=[]
        for name in ('before','after'):
            observation=args.out/(name+'.txt');observation.write_text('')
            r=subprocess.run([str(s)for s in[getattr(args,name),'-c','-U',image,args.out/'unused']],capture_output=True,
                             env=os.environ|{'DEADLIB_JPEG_FRAME_DUMP':str(observation.resolve())},timeout=60)
            assert not r.returncode and not r.stderr and b'warning'not in r.stdout.lower(),(source,name,r.returncode,r.stderr)
            outputs.append(observation.read_bytes())
        assert outputs[0]==outputs[1]and outputs[0],source
        rows.append({'source':directory+'/'+source.name,'source_sha256':hashlib.sha256(wire).hexdigest(),
                     'observation_sha256':hashlib.sha256(outputs[0]).hexdigest(),'C_EXP00_copy':fixed!=wire})
report={'scope':__doc__,'tools':hashes,'driver_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'cases':len(rows),'results':rows}
(args.out/'report.json').write_text(json.dumps(report,indent=2)+'\n')
print('Unchanged earlier C frame observations:',len(rows))
