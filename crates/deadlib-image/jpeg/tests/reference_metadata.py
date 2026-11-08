"""Independent libjpeg ICC/JFIF/Adobe and Pillow Exif metadata gate."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import PIL
from PIL import Image, ImageCms, features

parser=argparse.ArgumentParser()
parser.add_argument("--oracle",required=True,type=Path)
parser.add_argument("--native",required=True,type=Path)
parser.add_argument("--cjpeg",required=True,type=Path)
parser.add_argument("--out",required=True,type=Path)
args=parser.parse_args()
args.out.mkdir(parents=True,exist_ok=True)
root=Path(__file__).resolve().parent
fixture=root/"fixtures"/"metadata"
fixture.mkdir(exist_ok=True)
srgb=fixture/"srgb.icc"
if not srgb.exists(): srgb.write_bytes(ImageCms.ImageCmsProfile(ImageCms.createProfile("sRGB")).tobytes())
records=[]

def run(command):
    result=subprocess.run([str(v) for v in command],check=True,capture_output=True)
    assert not result.stderr,result.stderr.decode()
    return result

def check(path,label,profile=None):
    c=args.out/(label+"-c")
    native=args.out/(label+"-native")
    run([args.oracle,path,c]);run([args.native,"inspect",path,native])
    a=json.loads(c.with_suffix(".json").read_text());b=json.loads(native.with_suffix(".json").read_text())
    assert all(b[key]==value for key,value in a.items()),(label,a,b)
    assert c.with_suffix(".markers").read_bytes()==native.with_suffix(".markers").read_bytes(),label
    if profile is not None:
        assert c.with_suffix(".icc").read_bytes()==native.with_suffix(".icc").read_bytes()==profile,label
    records.append({"case":label,"sha256":hashlib.sha256(path.read_bytes()).hexdigest(),"profile_bytes":a["icc_length"],
                    "profile_sha256":hashlib.sha256(profile).hexdigest() if profile is not None else None,
                    "marker_match":True,"jfif_adobe_match":True,"orientation":b["orientation"]})
    return b

for mode in ("sequential","progressive"):
    for source in sorted((root/"fixtures"/mode).glob("*.jpg")):
        check(source,mode+"-"+source.stem)
for path in sorted(fixture.glob("ycck-*.jpg")): check(path,path.stem)

base=(root/"fixtures"/"sequential"/"8-gray.jpg").read_bytes()
def app(marker,payload): return bytes((255,marker))+struct.pack(">H",len(payload)+2)+payload
def after_jfif(jpeg,payload):
    end=4+int.from_bytes(jpeg[4:6],"big")
    return jpeg[:end]+payload+jpeg[end:]

rgb=(root/"fixtures"/"sequential"/"8-rgb.jpg").read_bytes()
conflict=args.out/"jfif-adobe-conflict.jpg"
conflict.write_bytes(rgb[:2]+app(0xe0,b"JFIF\0\x01\x02\0\0\x01\0\x01\0\0")+rgb[2:])
assert check(conflict,conflict.stem)["color_space"]=="YCbCr"
late=args.out/"late-adobe.jpg";color=(root/"fixtures"/"sequential"/"8-420.jpg").read_bytes()
late.write_bytes(color[:-2]+app(0xee,b"Adobe\0\x64\0\0\0\0\0")+color[-2:])
assert check(late,late.stem)["color_space"]=="YCbCr"

for endian in ("<",">"):
    exif=b"Exif\0\0"+(b"II" if endian=="<" else b"MM")+struct.pack(endian+"HIH",42,8,1)
    exif+=struct.pack(endian+"HHI",0x0112,3,1)+struct.pack(endian+"H",6)+b"\0\0"+struct.pack(endian+"I",0)
    path=args.out/("exif-"+("little" if endian=="<" else "big")+".jpg")
    path.write_bytes(after_jfif(base,app(0xe1,exif)))
    observed=check(path,path.stem)
    with Image.open(path) as image: assert image.getexif()[0x0112]==observed["orientation"]==6
    rewritten=args.out/(path.stem+"-rewritten.jpg")
    run([args.native,"rewrite",path,rewritten,"preserve",1]);check(rewritten,rewritten.stem)
    with Image.open(rewritten) as image: assert image.getexif()[0x0112]==6

density=bytearray(base);density[6+7]=1;density[6+8:6+10]=struct.pack(">H",300);density[6+10:6+12]=struct.pack(">H",240)
for label,payload in [
    ("jfxx-rgb",b"JFXX\0\x13\x02\x01"+bytes((0,1,2,253,254,255))),
    ("jfxx-indexed",b"JFXX\0\x11\x02\x01"+bytes(range(256))*3+bytes((0,255))),
    ("jfxx-jpeg",b"JFXX\0\x10"+(root/"fixtures"/"sequential"/"8-444.jpg").read_bytes())]:
    path=args.out/(label+".jpg");path.write_bytes(after_jfif(bytes(density),app(0xe0,payload)));check(path,label)

profiles=[("srgb",srgb.read_bytes())]+[(str(size),bytes(range(251))*(size//251)+bytes(range(size%251)))
    for size in (1,65519,65520,131038,131039,65519*255)]
for label,profile in profiles:
    profile_path=args.out/("profile-"+label+".icc");profile_path.write_bytes(profile)
    precisions=(8,) if len(profile)>1_000_000 else (8,12)
    for precision in precisions:
        reference=args.out/(f"reference-p{precision}-"+label+".jpg")
        run([args.cjpeg,"-precision",precision,"-icc",profile_path,"-outfile",reference,root/"fixtures"/"sequential"/f"source-{precision}.ppm"])
        check(reference,reference.stem,profile)
        for progressive in (0,1):
            path=args.out/(f"native-p{precision}-p{progressive}-"+label+".jpg")
            run([args.native,"rewrite",reference,path,profile_path,progressive]);check(path,path.stem,profile)
        path=args.out/(f"removed-p{precision}-"+label+".jpg")
        run([args.native,"rewrite",reference,path,"remove",0]);result=check(path,path.stem);assert not result["icc"]
        if label=="srgb": ImageCms.ImageCmsProfile(str((args.out/(reference.stem+"-native")).with_suffix(".icc")))

source_path=args.out/"reference-p8-131039.jpg"
jpeg=source_path.read_bytes();position=2;segments=[];icc_slots=[]
while jpeg[position:position+2]!=b"\xff\xda":
    end=position+2+int.from_bytes(jpeg[position+2:position+4],"big")
    payload=jpeg[position:end]
    if payload.startswith(b"\xff\xe2") and payload[4:].startswith(b"ICC_PROFILE\0"): icc_slots.append(len(segments))
    segments.append(payload);position=end
reordered=list(reversed([segments[i] for i in icc_slots]))
for i,payload in zip(icc_slots,reordered): segments[i]=payload
path=args.out/"reordered-icc.jpg";path.write_bytes(jpeg[:2]+b"".join(segments)+jpeg[position:])
check(path,path.stem,dict(profiles)["131039"])

# Both readers reject incomplete/duplicate/contradictory ICC embeddings, while
# the native JPEG decoder still parses their pixels (covered by Rust tests).
bad=[b"ICC_PROFILE\0\x01\x02abc",b"ICC_PROFILE\0\x00\x01abc",b"ICC_PROFILE\0\x02\x01abc",
     b"ICC_PROFILE\0\x01\x00abc"]
negative=0
for i,payloads in enumerate([[b] for b in bad]+[[bad[0],bad[0]],[bad[0],b"ICC_PROFILE\0\x02\x03def"]]):
    path=args.out/f"bad-icc-{i}.jpg";path.write_bytes(after_jfif(base,b"".join(app(0xe2,p) for p in payloads)))
    prefix=args.out/f"bad-icc-{i}-c"
    result=subprocess.run([str(args.oracle),str(path),str(prefix)],check=True,capture_output=True)
    assert not result.stderr or b"bad ICC marker" in result.stderr,result.stderr.decode()
    assert not json.loads(prefix.with_suffix(".json").read_text())["icc"]
    result=subprocess.run([str(args.native),"inspect",str(path),str(args.out/f"bad-icc-{i}-native")],capture_output=True)
    assert result.returncode!=0;negative+=1

report={"reference":"libjpeg-turbo 3.2.1 metadata API","pillow":PIL.__version__,"littlecms":features.version("littlecms2"),
        "valid_cases":len(records),"malformed_icc_cases":negative,"srgb_sha256":hashlib.sha256(srgb.read_bytes()).hexdigest(),"results":records}
(args.out/"report.json").write_text(json.dumps(report,indent=2)+"\n")
print(json.dumps({k:v for k,v in report.items() if k!="results"}))
