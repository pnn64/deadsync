"""Native color/upsampling comparison with libjpeg and an independent rational filter."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
from fractions import Fraction
from PIL import Image

parser=argparse.ArgumentParser()
parser.add_argument("--oracle",required=True,type=Path)
parser.add_argument("--native",required=True,type=Path)
parser.add_argument("--pixel-corpus",required=True,type=Path)
parser.add_argument("--extra-corpus",required=True,type=Path)
parser.add_argument("--out",required=True,type=Path)
args=parser.parse_args();args.out.mkdir(parents=True,exist_ok=True)
fixtures=Path(__file__).resolve().parent/"fixtures"

def run(command):
    result=subprocess.run([str(v) for v in command],check=True,capture_output=True)
    assert not result.stderr,result.stderr.decode()
def u16(path):
    data=path.read_bytes();return list(struct.unpack("<"+"H"*(len(data)//2),data))
def structure(path):
    data=path.read_bytes();precision,width,height,count=struct.unpack_from("<4I",data,4);position=20;components=[]
    for _ in range(count):
        geometry=struct.unpack_from("<7I",data,position);position+=28+128+geometry[5]*geometry[6]*128
        components.append(geometry)
    return precision,width,height,components
def error(a,b):
    assert len(a)==len(b)
    e=[abs(x-y) for x,y in zip(a,b)];return {"max_error":max(e),"mean_error":sum(e)/len(e)}

def rational_reference(planes,geometry,precision,width,height,method):
    offsets=[];offset=0
    for c in geometry: offsets.append(offset);offset+=c[3]*c[4]
    maxh=max(c[1] for c in geometry);maxv=max(c[2] for c in geometry);maximum=(1<<precision)-1
    def component(ci,x,y):
        c=geometry[ci];get=lambda a,b: planes[offsets[ci]+max(0,min(b,c[4]-1))*c[3]+max(0,min(a,c[3]-1))]
        sx=Fraction((2*x+1)*c[1],2*maxh)-Fraction(1,2)
        sy=Fraction((2*y+1)*c[2],2*maxv)-Fraction(1,2)
        if method=="nearest": return get((sx+Fraction(1,2)).__floor__(),(sy+Fraction(1,2)).__floor__())
        ix=sx.__floor__();iy=sy.__floor__();fx=sx-ix;fy=sy-iy
        value=get(ix,iy)*(1-fx)*(1-fy)+get(ix+1,iy)*fx*(1-fy)+get(ix,iy+1)*(1-fx)*fy+get(ix+1,iy+1)*fx*fy
        return (value+Fraction(1,2)).__floor__()
    result=[]
    for y in range(height):
        for x in range(width):
            l,cb,cr=[component(i,x,y) for i in range(3)];cb-=1<<(precision-1);cr-=1<<(precision-1)
            # Independent published JFIF real-valued matrix; rounding errors
            # are bounded separately from the rational interpolation.
            result.extend(max(0,min(int(v+0.5),maximum)) for v in (l+1.402*cr,l-.344136286*cb-.714136286*cr,l+1.772*cb))
    return result

sources=[];seen=set()
for path in [*sorted((fixtures/"sequential").glob("*.jpg")),*sorted((fixtures/"progressive").glob("*.jpg")),
             *sorted((fixtures/"metadata").glob("ycck-*.jpg")),*sorted(args.pixel_corpus.glob("p*-q*.jpg")),*sorted(args.extra_corpus.glob("*.jpg"))]:
    # The pixel corpus also holds reference-encoded JPEGs; include only native
    # cases listed by the generator, not outputs from prior oracle runs.
    if "-reference" in path.name: continue
    digest=hashlib.sha256(path.read_bytes()).hexdigest()
    if digest in seen: continue
    seen.add(digest);sources.append((path,digest))
records=[]
for index,(path,digest) in enumerate(sources):
    ref=args.out/f"case-{index}-reference";run([args.oracle,path,ref])
    precision,width,height,geometry=structure(ref.with_suffix(".coef"));maximum=(1<<precision)-1
    count=len(geometry);ink=count==4;ycck="ycck" in path.name
    conventional="untagged" in path.name
    ratios=[(max(c[1] for c in geometry)//c[1],max(c[2] for c in geometry)//c[2]) for c in geometry]
    fractional=any(max(c[1] for c in geometry)%c[1] or max(c[2] for c in geometry)%c[2] for c in geometry)
    triangle_match=not fractional and all(h in (1,2) and v in (1,2) for h,v in ratios)
    for method in ("nearest","linear"):
        native=args.out/f"case-{index}-{method}";run([args.native,path,native,method]);pixels=u16(native.with_suffix(".rgb"))
        color_ref=ref
        if method=="linear" and triangle_match:
            color_ref=args.out/f"case-{index}-fancy";run([args.oracle,path,color_ref,"fancy"])
        independent=fractional or (method=="linear" and not triangle_match)
        if independent:
            assert not ink and count==3
            expected=rational_reference(u16(ref.with_suffix(".planes")),geometry,precision,width,height,method)
            backend="libjpeg float planes + independent rational/JFIF conversion"
        else:
            expected=u16(color_ref.with_suffix(".color"));backend="libjpeg float/"+("fancy" if method=="linear" else "nearest")
        record={"file":path.name,"sha256":digest,"precision":precision,"method":method,"reference":backend}
        if ink:
            cmyk=u16(native.with_suffix(".cmyk"));result=error(cmyk,expected);limit=6 if method=="linear" else 4 if ycck else 1
            assert result["max_error"]<=limit,(path.name,method,"cmyk",result)
            record["cmyk"]=result
            inks=u16(native.with_suffix(".ink"));assert inks==([maximum-v for v in cmyk] if not conventional else cmyk)
            expected_rgb=[]
            for i in range(0,len(expected),4):
                c,m,y,k=expected[i:i+4]
                if conventional: c,m,y,k=maximum-c,maximum-m,maximum-y,maximum-k
                expected_rgb.extend((v*k+maximum//2)//maximum for v in (c,m,y))
            expected=expected_rgb
        result=error(pixels,expected)
        interpolated=method=="linear" and any(h!=1 or v!=1 for h,v in ratios)
        tolerance=8 if ycck and interpolated else 6 if ycck else 6 if interpolated else 3 if ink else 1 if count==1 or all(c[0] in b"RGB" for c in geometry) else 4
        assert result["max_error"]<=tolerance,(path.name,method,result,tolerance)
        record.update(result);record["tolerance"]=tolerance
        if ink and precision==8:
            with Image.open(path) as image: pillow=list(image.convert("RGB").tobytes())
            result=error(pixels,pillow)
            if not conventional and method=="linear": assert result["max_error"]<=8,(path.name,method,"pillow",result)
            record["pillow_rgb"]=result
            if conventional: record["pillow_polarity_policy"]="different; C raw components use explicit conventional ink policy"
        records.append(record)
report={"reference":"libjpeg-turbo 3.2.1 float IDCT; see per-case interpolation policy","unique_images":len(sources),
        "comparisons":len(records),"results":records}
(args.out/"report.json").write_text(json.dumps(report,indent=2)+"\n")
print(json.dumps({k:v for k,v in report.items() if k!="results"}))
