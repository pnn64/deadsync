import hashlib, pathlib, subprocess, sys

root = pathlib.Path(__file__).resolve().parent
stages = sys.argv[1:3]
cases = sys.argv[3].split(",")
modes = sys.argv[4].split(",")
temp = root / "verify.yuv"
try:
    for clip in cases:
        reference = subprocess.check_output(["ffmpeg","-v","error","-nostdin","-i",str(root / (clip+".h264")),"-map","0:v:0","-pix_fmt","yuv420p","-f","hash","-hash","sha256","-"],text=True,stderr=subprocess.DEVNULL).strip().split("=")[-1]
        for mode in modes:
            p = root / (clip+(".avcc" if mode.endswith("avcc") else ".h264"))
            hashes = []
            for stage in stages:
                subprocess.check_output([str(root / f"{stage}-decode-cycles.exe"),"42",mode,str(p),"1",str(temp)])
                with temp.open("rb") as f: hashes.append(hashlib.file_digest(f,"sha256").hexdigest())
            assert hashes[0] == hashes[1] == reference, (clip,mode,hashes,reference)
            print(clip,mode,reference,flush=True)
finally:
    temp.unlink(missing_ok=True)
