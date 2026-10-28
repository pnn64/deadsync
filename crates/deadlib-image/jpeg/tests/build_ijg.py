"""Build the pinned IJG 10 public coefficient oracle with a 255-component cap."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import tarfile
import urllib.request

parser=argparse.ArgumentParser()
parser.add_argument("--work",required=True,type=Path)
args=parser.parse_args()
args.work.mkdir(parents=True,exist_ok=True)
archive=args.work/"jpegsrc.v10.tar.gz"
if not archive.exists():
    archive.write_bytes(urllib.request.urlopen("https://www.ijg.org/files/jpegsrc.v10.tar.gz",timeout=45).read())
digest=hashlib.sha256(archive.read_bytes()).hexdigest()
assert digest=="8b9eaa13242690ebd03e1728ab1edf97a81a78ed6e83624d493655f31ac95ab5"
source=args.work/"jpeg-10"
if not source.exists():
    with tarfile.open(archive) as tar:tar.extractall(args.work,filter="data")
header=source/"jmorecfg.h"
text=header.read_text()
assert "#define MAX_COMPONENTS  10" in text or "#define MAX_COMPONENTS  255" in text
header.write_text(text.replace("#define MAX_COMPONENTS  10", "#define MAX_COMPONENTS  255"))
shutil.copy2(source/"jconfig.vc",source/"jconfig.h")
build=args.work/"build"
subprocess.run(["cmake","-S",str(Path(__file__).parent/"oracle/ijg"),"-B",str(build),
                "-DJPEG_SOURCE_DIR="+str(source.resolve())],check=True)
subprocess.run(["cmake","--build",str(build),"--config","Release","--target","ijg-coefficients","--","/m:1","/verbosity:minimal"],check=True)
exe=build/"Release/ijg-coefficients.exe"
report={"reference":"IJG libjpeg 10","source":"https://www.ijg.org/files/jpegsrc.v10.tar.gz","archive_sha256":digest,
        "configuration":"jconfig.vc; jmorecfg.h MAX_COMPONENTS raised from 10 to 255; all codec algorithms unchanged; coefficient API only",
        "tool_sha256":hashlib.sha256(exe.read_bytes()).hexdigest(),"configured_header_sha256":hashlib.sha256(header.read_bytes()).hexdigest()}
(args.work/"build.json").write_text(json.dumps(report,indent=2)+"\n")
print(json.dumps(report))
