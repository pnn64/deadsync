"""External coefficient encoder gate against the pinned C oracle; no Python codec."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

parser=argparse.ArgumentParser()
parser.add_argument("--oracle",required=True,type=Path)
parser.add_argument("--native",required=True,type=Path)
parser.add_argument("--out",required=True,type=Path)
args=parser.parse_args()
args.out.mkdir(parents=True,exist_ok=True)
root=Path(__file__).resolve().parent/"fixtures"
records=[]
for mode in ("sequential","progressive"):
    for source in sorted((root/mode).glob("*.jpg")):
        reference=source.with_suffix(".coef").read_bytes()
        for progressive in (0,1):
            for optimize in (0,1):
                for restart in (0,1,7):
                    name=f"{mode}-{source.stem}-p{progressive}-o{optimize}-r{restart}"
                    image=args.out/(name+".jpg")
                    subprocess.run([str(args.native),str(source),str(image),str(progressive),str(optimize),str(restart)],check=True,capture_output=True)
                    prefix=args.out/name
                    result=subprocess.run([str(args.oracle),str(image),str(prefix)],check=True,capture_output=True)
                    assert not result.stderr, (name,result.stderr.decode())
                    assert prefix.with_suffix(".coef").read_bytes()==reference,name
                    records.append({"case":name,"bytes":image.stat().st_size,"sha256":hashlib.sha256(image.read_bytes()).hexdigest(),"coefficients":"exact"})
report={"cases":len(records),"reference":"libjpeg-turbo 3.2.1 (see tests/oracle/README.md)","results":records}
(args.out/"report.json").write_text(json.dumps(report,indent=2)+"\n")
print(json.dumps({"cases":len(records),"coefficient_match":"exact","reference_warnings":0}))
