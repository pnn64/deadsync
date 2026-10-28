"""Native pixel encoder/C decoder and float-FDCT comparison. Standard library only."""
import argparse
import csv
import hashlib
import json
import math
from pathlib import Path
import struct
import subprocess

parser=argparse.ArgumentParser()
parser.add_argument("--oracle",required=True,type=Path)
parser.add_argument("--cjpeg",required=True,type=Path)
parser.add_argument("--corpus",required=True,type=Path)
args=parser.parse_args()

def run(command):
    result=subprocess.run([str(v) for v in command],check=True,capture_output=True)
    assert not result.stderr, result.stderr.decode()

def coefficients(path):
    data=path.read_bytes()
    assert data[:4]==b"JCO1"
    header=struct.unpack_from("<4I",data,4)
    pos=20
    components=[]
    for _ in range(header[3]):
        geometry=struct.unpack_from("<7I",data,pos);pos+=28
        quant=struct.unpack_from("<64H",data,pos);pos+=128
        count=geometry[5]*geometry[6]*64
        blocks=struct.unpack_from("<"+"h"*count,data,pos);pos+=count*2
        components.append((geometry,quant,blocks))
    assert pos==len(data)
    return header,components

def source(precision,channels):
    maximum=(1<<precision)-1
    return [((x*193+y*317+c*911+(x*y*53)%997)*maximum//4095)&maximum
            for y in range(25) for x in range(31) for c in range(channels)]

def quality(path,precision,original):
    data=path.read_bytes()
    values=struct.unpack("<"+"H"*(len(data)//2),data)
    assert len(values)==len(original)
    mse=sum((a-b)**2 for a,b in zip(values,original))/len(values)
    return {"mse":mse,"psnr_db":10*math.log10(((1<<precision)-1)**2/mse) if mse else None}

records=[]
sampling={"444":"1x1,1x1,1x1","422":"2x1,1x1,1x1","420":"2x2,1x1,1x1",
          "411":"4x1,1x1,1x1","440":"1x2,1x1,1x1","410":"4x2,1x1,1x1"}
with (args.corpus/"manifest.tsv").open() as file:
    cases=list(csv.DictReader(file,delimiter="\t"))
for case in cases:
    name=case["case"]
    path=args.corpus/(name+".jpg")
    prefix=args.corpus/(name+"-oracle")
    run([args.oracle,path,prefix])
    expected=(args.corpus/(name+"-expected.coef")).read_bytes()
    assert prefix.with_suffix(".coef").read_bytes()==expected,(name,"entropy coefficient mismatch")
    record={"case":name,"native_bytes":path.stat().st_size,"sha256":hashlib.sha256(path.read_bytes()).hexdigest(),"entropy_coefficients":"exact"}
    precision=int(case["precision"])
    channels=1 if case["color"]=="gray" else 4 if case["color"] in ("cmyk","ycck") else 3
    if prefix.with_suffix(".color").exists() and case["color"]!="ycck":
        record["native_quality"]=quality(prefix.with_suffix(".color"),precision,source(precision,channels))
    if case["source"]!="-" and case["sampling"] in sampling:
        reference=args.corpus/(name+"-reference.jpg")
        command=[args.cjpeg,"-precision",precision,"-quality",case["quality"],"-dct","float","-optimize"]
        if case["color"]=="rgb": command += ["-rgb"]
        elif case["color"]=="gray": command += ["-grayscale"]
        else: command += ["-sample",sampling[case["sampling"]]]
        if case["baseline"]=="1": command += ["-baseline"]
        if case["progressive"]=="1": command += ["-progressive"]
        command += ["-restart","7B" if case["progressive"]=="1" else "3B","-outfile",reference,args.corpus/case["source"]]
        # Wide quantization emits an informational baseline-compatibility message.
        result=subprocess.run([str(v) for v in command],check=True,capture_output=True)
        assert not result.stderr or b"too coarse for baseline JPEG" in result.stderr,result.stderr.decode()
        ref_prefix=args.corpus/(name+"-reference-oracle")
        run([args.oracle,reference,ref_prefix])
        a,ac=coefficients(args.corpus/(name+"-expected.coef"))
        b,bc=coefficients(ref_prefix.with_suffix(".coef"))
        assert a==b,(name,"frame mismatch")
        errors=[]
        for (ag,aq,av),(bg,bq,bv) in zip(ac,bc):
            assert ag==bg and aq==bq,(name,"quantizer/geometry mismatch")
            errors += [abs(x-y) for x,y in zip(av,bv)]
        maximum=max(errors)
        # Gray/RGB have identical source samples. YCbCr rounding and box
        # downsampling can differ by one sample; an orthonormal 8x8 DCT maps
        # that to <=8 coefficient levels, plus <=1 quantization/FDCT rounding.
        tolerance=1 if case["color"] in ("gray","rgb") else 9
        assert maximum<=tolerance,(name,maximum,tolerance)
        record.update({"reference_bytes":reference.stat().st_size,"size_ratio":path.stat().st_size/reference.stat().st_size,
                       "max_fdct_coefficient_error":maximum,"fdct_tolerance":tolerance,
                       "mean_fdct_coefficient_error":sum(errors)/len(errors),
                       "reference_quality":quality(ref_prefix.with_suffix(".color"),precision,source(precision,channels))})
    records.append(record)
report={"reference":"libjpeg-turbo 3.2.1, float DCT/IDCT, no SIMD; correctness/size/quality only",
        "cases":len(records),"coefficient_matches":len(records),
        "fdct_comparisons":sum("max_fdct_coefficient_error" in r for r in records),"results":records}
(args.corpus/"report.json").write_text(json.dumps(report,indent=2)+"\n")
print(json.dumps({k:v for k,v in report.items() if k!="results"}))
