"""Independent C CMYK/YCCK encoding and native coefficient entropy gate."""
import argparse
import csv
import hashlib
import json
from pathlib import Path
import struct
import subprocess
parser=argparse.ArgumentParser()
parser.add_argument("--oracle",required=True,type=Path)
parser.add_argument("--encoder",required=True,type=Path)
parser.add_argument("--corpus",required=True,type=Path)
args=parser.parse_args()
def run(command):
    result=subprocess.run([str(x) for x in command],check=True,capture_output=True)
    assert not result.stderr,result.stderr.decode()
def read(path):
    data=path.read_bytes();header=struct.unpack_from("<4I",data,4);position=20;components=[]
    for _ in range(header[3]):
        shape=struct.unpack_from("<7I",data,position);position+=28
        table=struct.unpack_from("<64H",data,position);position+=128
        count=shape[5]*shape[6]*64;values=struct.unpack_from("<"+"h"*count,data,position);position+=count*2
        components.append((shape,table,values))
    assert position==len(data);return header,components
records=[]
with (args.corpus/"manifest.tsv").open() as file: cases=list(csv.DictReader(file,delimiter="\t"))
for case in cases:
    name=case["case"];path=args.corpus/(name+".jpg");prefix=args.corpus/(name+"-c")
    run([args.oracle,path,prefix])
    expected=args.corpus/(name+"-expected.coef")
    assert expected.read_bytes()==prefix.with_suffix(".coef").read_bytes(),(name,"native entropy")
    reference=args.corpus/(name+"-reference.jpg")
    run([args.encoder,args.corpus/case["source"],reference,31,25,case["precision"],case["quality"],case["ycck"],case["progressive"],*case["factors"].split(",")])
    refprefix=args.corpus/(name+"-reference-c");run([args.oracle,reference,refprefix])
    a,ac=read(expected);b,bc=read(refprefix.with_suffix(".coef"));assert a==b,(name,"frame")
    differences=[]
    for (ag,aq,av),(bg,bq,bv) in zip(ac,bc):
        assert ag==bg and aq==bq,(name,"geometry/quantizers",ag,bg)
        differences.extend(abs(x-y) for x,y in zip(av,bv))
    bound=1 if case["ycck"]=="0" and case["sampling"]=="444" else 9
    assert max(differences)<=bound,(name,max(differences),bound)
    partner=name.replace("-inverse-","-ink-") if case["polarity"]=="inverse" else name.replace("-ink-","-inverse-")
    counterpart=args.corpus/(partner+".jpg")
    assert counterpart.read_bytes()==path.read_bytes(),(name,"polarity changed emitted JPEG")
    records.append({"case":name,"sha256":hashlib.sha256(path.read_bytes()).hexdigest(),"entropy_coefficients":"exact",
                    "source_polarities_identical":True,"max_coefficient_error":max(differences),"mean_coefficient_error":sum(differences)/len(differences),
                    "tolerance":bound,"native_bytes":path.stat().st_size,"reference_bytes":reference.stat().st_size})
report={"reference":"libjpeg-turbo 3.2.1 float CMYK/YCCK encoder and decoder","cases":len(records),"results":records}
(args.corpus/"report.json").write_text(json.dumps(report,indent=2)+"\n")
print(json.dumps({"cases":len(records),"native_entropy":"exact","polarity_pairs":"byte-identical","max_fdct_error":max(r["max_coefficient_error"] for r in records)}))
