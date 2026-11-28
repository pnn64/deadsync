import csv, subprocess, sys, pathlib, os

root = pathlib.Path(__file__).resolve().parent
stages = sys.argv[1:3]
rounds = int(sys.argv[3])
cases = sys.argv[4].split(",")
modes = sys.argv[5].split(",")
cpu = int(sys.argv[7]) if len(sys.argv) > 7 else 42
def call(stage, kind, mode, clip, repeat, out=None):
    ext = {"bin":".bins","bypass":".bins","sign":".bins","cabres":".residual","init":".init","idct4":".idct4","idct8":".idct8","dq4":".dq4","dq8":".dq8","chroma4":".chroma4"}.get(mode, ".avcc" if mode.endswith("avcc") else ".h264")
    p = root / (clip + ext)
    exe = f"{stage}-decode-{kind}.exe" if mode.startswith(("surface", "packed")) else (f"{stage}-transform.exe" if mode.startswith(("idct","dq","chroma")) else f"{stage}-replay.exe")
    cmd = [str(root / exe), str(cpu), mode, str(p), str(repeat)]
    if out is not None: cmd.append(str(out))
    return list(map(int, subprocess.check_output(cmd, text=True).strip().split(",")))

suffix = "-" + sys.argv[6] + "-" + cases[0] + "-" + cases[-1] if len(sys.argv) > 6 else ""
output = root / ("-".join(stages) + "-" + "-".join(modes) + suffix + ".csv")
with output.open("w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["case","mode","kind","round","stage","repeats","units","signature","cycles","calls","bytes"])
    for clip in cases:
        for mode in modes:
            isolated = not mode.startswith(("surface","packed"))
            repeats = int(os.environ.get("H264_REPEATS", "256")) if isolated else 1
            for kind, n in ([("cycles", rounds)] if isolated else [("cycles", rounds), ("alloc", 2)]):
                for i in range(n):
                    pair = []
                    for stage in stages[::1 if i % 2 == 0 else -1]:
                        row = call(stage, kind, mode, clip, repeats)
                        w.writerow([clip,mode,kind,i,stage,repeats,*row]); f.flush()
                        pair.append(row)
                    assert pair[0][:2] == pair[1][:2], (clip,mode,pair)
            print(clip, mode, "done", flush=True)
