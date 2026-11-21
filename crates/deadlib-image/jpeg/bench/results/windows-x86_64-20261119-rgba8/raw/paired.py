"""Interleave frozen native JPEG builds per file; run only when authorized.
Example: paired.py baseline cmyk vertical --run paired-stages --ms 80 --rounds 2
Outputs: LABEL-NAME-roundN.tsv/.log and LABEL-paired.jsonl (exclusive creation).
"""
import argparse, hashlib, json, os, subprocess, time
from contextlib import ExitStack
from pathlib import Path

ROOT, OUT = Path(__file__).resolve().parents[2], Path(__file__).resolve().parent
HEADER = "file\tcodec\tpixels\tns\tprocess_cycles\talloc_calls\trequested_bytes"

def sha(path):
    with path.open("rb") as stream: return hashlib.file_digest(stream, "sha256").hexdigest()

def text(path):
    data = path.read_bytes()
    return data.decode("utf-16" if data.startswith((b"\xff\xfe", b"\xfe\xff")) else "utf-8-sig")

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("names", nargs="+", metavar="NAME")
parser.add_argument("--run", required=True, metavar="LABEL")
parser.add_argument("--ms", type=int, default=80)
parser.add_argument("--files-from", action="append")
parser.add_argument("--rounds", type=int, default=2)
args = parser.parse_args()
if args.ms <= 0 or args.rounds <= 0 or len(set(args.names)) != len(args.names):
    parser.error("positive --ms/--rounds and distinct NAME values are required")
if any(not value.replace("-", "").replace("_", "").isalnum() for value in args.names + [args.run]):
    parser.error("NAME and LABEL may contain only letters, numbers, hyphens and underscores")
lists = [(ROOT / p).resolve() for p in (args.files_from or [OUT / "jpeg-corpus-speed.txt", OUT / "jpeg-corpus-kodak.txt"])]
listed = [(ROOT / line.strip()).resolve() for p in lists for line in text(p).splitlines() if line.strip() and not line.lstrip().startswith("#")]
files = sorted(set(listed), key=lambda p: p.relative_to(ROOT).as_posix().casefold())
if not files: parser.error("the input lists contain no files")
items = json.loads(text(OUT / "jpeg-corpus-selection.json"))["speed_candidates"] + json.loads(text(OUT / "jpeg-corpus-kodak.json"))["inputs"]
metadata = {(ROOT / item["path"]).resolve(): item for item in items}
if args.files_from is None and (len(files) != 105 or set(files) != set(metadata)):
    parser.error("default lists no longer match the fixed 105-input manifest")
builds, inputs, pixels = {}, {}, {}
for name in args.names:
    build, exe = OUT / f"{name}-build.json", OUT / f"{name}.exe"
    identity, digest = json.loads(text(build)), sha(exe)
    if digest != identity["binaries"][exe.name]:
        raise RuntimeError(f"frozen executable changed: {exe}")
    builds[name] = dict(build=build.name, build_sha256=sha(build), exe_sha256=digest, identity=identity)
for path in files:
    item, digest = metadata.get(path), sha(path)
    if item and digest != item["sha256"]:
        raise RuntimeError(f"fixed input changed: {path}")
    inputs[path.relative_to(ROOT).as_posix()] = dict(sha256=digest, bytes=path.stat().st_size, manifest=item is not None)
    if item:
        pixels[path] = item["width"] * item["height"]
stems = {(r, n): OUT / f"{args.run}-{n}-round{r + 1}" for r in range(args.rounds) for n in args.names}
journal = OUT / f"{args.run}-paired.jsonl"
targets = [journal] + [Path(str(stem) + suffix) for stem in stems.values() for suffix in (".tsv", ".log")]
if any(path.exists() for path in targets):
    raise RuntimeError("refusing to overwrite paired-run output: " + ", ".join(str(p) for p in targets if p.exists()))
env = dict(os.environ, RAYON_NUM_THREADS="1", JPEG_BENCH_MS=str(args.ms))
for variable in ("JPEG_SNAPSHOT_DIR", "JPEG_BASELINE_DIR"): env.pop(variable, None)
started = time.time()
with ExitStack() as stack:
    record = stack.enter_context(journal.open("x", encoding="utf-8", newline="\n"))
    def log(value):
        record.write(json.dumps(value) + "\n")
        record.flush()
    outputs = {key: stack.enter_context(Path(str(stem) + ".tsv").open("x", encoding="utf-8", newline="\n")) for key, stem in stems.items()}
    errors = {key: stack.enter_context(Path(str(stem) + ".log").open("xb")) for key, stem in stems.items()}
    for stream in outputs.values():
        stream.write(HEADER + "\n")
        stream.flush()
    log(dict(type="plan", runner_sha256=sha(Path(__file__)), options=vars(args), builds=builds, inputs=inputs, lists={str(p): sha(p) for p in lists}, listed_count=len(listed), unique_count=len(files), started_unix=started, order="rotate versions by file index; reverse on odd zero-based rounds", environment={k: env[k] for k in ("RAYON_NUM_THREADS", "JPEG_BENCH_MS")}))
    try:
        for r in range(args.rounds):
            for index, path in enumerate(files):
                offset = index % len(args.names)
                order = args.names[offset:] + args.names[:offset]
                if r % 2: order.reverse()
                for name in order:
                    command = [str(OUT / f"{name}.exe"), "--rgba8", "--native-only", path.relative_to(ROOT).as_posix()]
                    clock = time.perf_counter()
                    result = subprocess.run(command, cwd=ROOT, env=env, capture_output=True, timeout=120)
                    log(dict(type="invocation", round=r + 1, file_index=index, name=name, order=order, command=command, elapsed_seconds=time.perf_counter() - clock, exit_code=result.returncode))
                    errors[r, name].write(("\nFILE\t" + command[-1] + "\n").encode() + result.stderr + b"\nSTDOUT\n" + result.stdout)
                    errors[r, name].flush()
                    lines = result.stdout.decode("utf-8-sig").splitlines()
                    if result.returncode or len(lines) != 2 or lines[0] != HEADER:
                        raise RuntimeError(f"exit/header/row-count failure: {name} {command[-1]}")
                    fields = lines[1].split("\t")
                    if len(fields) != 7 or fields[1] != "native" or (ROOT / fields[0]).resolve() != path:
                        raise RuntimeError(f"row path/codec mismatch: {name} {command[-1]}")
                    count, ns, cycles, calls, requested = map(int, fields[2:])
                    if min(count, ns, cycles) <= 0 or min(calls, requested) < 0 or count != pixels.setdefault(path, count):
                        raise RuntimeError(f"row numeric/pixel mismatch: {name} {command[-1]}")
                    outputs[r, name].write(lines[1] + "\n")
                    outputs[r, name].flush()
                if (index + 1) % 10 == 0 or index + 1 == len(files):
                    print(f"round {r + 1}/{args.rounds}: {index + 1}/{len(files)} files", flush=True)
    except BaseException as error:
        log(dict(type="failure", error=repr(error), elapsed_seconds=time.time() - started))
        raise
    log(dict(type="complete", elapsed_seconds=time.time() - started))
