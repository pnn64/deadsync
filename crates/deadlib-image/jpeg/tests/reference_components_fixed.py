"""Targeted fixed-Huffman checks for large direct frames and source pyramids."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
from hierarchy_support import structure,check_frame_data
from stream_support import clear_stale_exp
parser=argparse.ArgumentParser()
for name in ('author','ijg','reference','out'):parser.add_argument('--'+name,required=True,type=Path)
args=parser.parse_args();args.out.mkdir(parents=True,exist_ok=True)
tools=args.out/'tools';tools.mkdir(exist_ok=True);hashes={}
for name in ('author','ijg','reference'):
    source=getattr(args,name);target=tools/(name+source.suffix);shutil.copy2(source,target)
    hashes[name]=hashlib.sha256(target.read_bytes()).hexdigest();setattr(args,name,target.resolve())
def run(command,env=None):
    result=subprocess.run([str(s)for s in command],capture_output=True,env=env,timeout=60)
    assert not result.returncode and not result.stderr and b'warning'not in result.stdout.lower(),(command,result.returncode,result.stderr)
prefix=args.out/'image';image=args.out/'image.jpg';rows=[]
for count in (5,17,255):
    for precision in (8,12):
        for mode in (0,1,2):
            run([args.author,prefix,count,precision,0,3,0,1,3,0,0,mode,0,0])
            wire=image.read_bytes();_,frames,_,_,_=structure(wire,False)
            expected=[(args.out/f'image.expected.frame-{j}.{"samples"if f["marker"]&3==3 else"coef"}').read_bytes()for j,f in enumerate(frames)]
            if not mode:
                run([args.ijg,image,args.out/'ijg.coef']);assert(args.out/'ijg.coef').read_bytes()==expected[0]
            fixed=clear_stale_exp(wire,frames);image.write_bytes(fixed)
            observation=args.out/'frames.txt';observation.write_text('')
            run([args.reference,'-c','-U',image,args.out/'unused'],os.environ|{'DEADLIB_JPEG_FRAME_DUMP':str(observation.resolve())})
            blocks,samples=check_frame_data(frames,expected,observation.read_text())
            rows.append({'count':count,'precision':precision,'mode':mode,'C_blocks':blocks,'C_samples':samples,
                         'C_EXP00_copy':fixed!=wire,'sha256':hashlib.sha256(wire).hexdigest()})
report={'scope':__doc__,'tools':hashes,'driver_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'cases':len(rows),'results':rows}
(args.out/'report.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items()if k!='results'}))
