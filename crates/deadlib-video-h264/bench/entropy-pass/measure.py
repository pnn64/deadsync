import csv, subprocess, sys, pathlib, os

root = pathlib.Path(__file__).resolve().parent
stages = sys.argv[1:3]
rounds = int(sys.argv[3])
cases = sys.argv[4].split(",")
modes = sys.argv[5].split(",")
cpu = int(sys.argv[7]) if len(sys.argv) > 7 else 42
def call(stage, kind, mode, clip, repeat, out=None):
    ext = ".trace" if mode in ("residual", "ue", "bits") else (".avcc" if mode.endswith("avcc") else ".h264")
    p = root / (clip + ext)
    exe = f"{stage}-{kind}.exe"
    if mode.startswith(("surface", "packed")) and os.environ.get("H264_MIXED_BENCH") != "1":
        exe = f"{stage}-decode-{kind}.exe"
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
            repeats = int(os.environ.get("H264_REPEATS", "64")) if mode.startswith("parse") or mode in ("residual", "ue", "bits") else 1
            if mode == "peek" and clip != "cavlc": continue
            for kind, n in [("cycles", rounds), ("alloc", 2)]:
                for i in range(n):
                    pair = []
                    for stage in stages[::1 if i % 2 == 0 else -1]:
                        row = call(stage, kind, mode, clip, repeats)
                        w.writerow([clip,mode,kind,i,stage,repeats,*row]); f.flush()
                        pair.append(row)
                    assert pair[0][:2] == pair[1][:2], (clip,mode,pair)
            print(clip, mode, "done", flush=True)
