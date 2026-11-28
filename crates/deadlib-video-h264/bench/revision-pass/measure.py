import csv, subprocess, sys, pathlib, statistics

root = pathlib.Path(__file__).resolve().parent
stages = sys.argv[1:3]
rounds = int(sys.argv[3])
cases = sys.argv[4].split(",")
modes = sys.argv[5].split(",")
cpu = 42
def call(stage, kind, mode, clip, repeat, out=None):
    p = root / (clip + (".avcc" if mode.endswith("avcc") else ".h264"))
    cmd = [str(root / f"{stage}-{kind}.exe"), str(cpu), mode, str(p), str(repeat)]
    if out is not None: cmd.append(str(out))
    return list(map(int, subprocess.check_output(cmd, text=True).strip().split(",")))

output = root / ("-".join(stages) + "-" + "-".join(modes) + ".csv")
with output.open("w", newline="") as f:
    w = csv.writer(f)
    w.writerow(["case","mode","kind","round","stage","repeats","units","signature","cycles","calls","bytes"])
    for clip in cases:
        for mode in modes:
            repeats = 64 if mode.startswith("parse") else 1
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
