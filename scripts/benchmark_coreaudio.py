"""Archive and check the actual CoreAudio timing code without Apple frameworks.

Build before editing, then archive each subsequent stage under another name.
Use benchmark_sampling.py to compare the resulting timing.exe files on Windows.
Its cycles/op and ns/op describe batches of 256 conversions or timing updates.
"""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import subprocess


ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / "crates/deadlib-audio-backend-coreaudio/src/lib.rs"


def function(source, name):
    start = source.index(f"fn {name}(")
    opening = source.index("{", start)
    depth = 1
    end = opening + 1
    while depth:
        depth += (source[end] == "{") - (source[end] == "}")
        end += 1
    return "#[inline(always)]\n" + source[start:end]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", required=True, type=Path)
    parser.add_argument("--source", type=Path, default=SOURCE)
    parser.add_argument("--debug", action="store_true")
    args = parser.parse_args()
    destination = args.archive.resolve()
    destination.mkdir(parents=True, exist_ok=True)
    source = args.source.read_text(encoding="utf-8")
    timing = source[source.index("            let period_frames = args.num_frames"):
                    source.index("            publish_output_timing_quality(quality);")]
    timing = timing.replace("args.num_frames", "num_frames")
    timing = timing.replace("publish_output_timing(", "(").rstrip().removesuffix(";")
    generated = "\n\n".join(function(source, name) for name in
                              ("scale_mach_time", "frames_to_nanos", "nanos_to_frames"))
    generated += """
#[allow(unused_variables)]
#[inline(always)]
fn callback_timing(sample_rate_hz: u32, buffer_frames: u32, buffer_ns: u64,
                   num_frames: usize, downstream_latency_ns: u64)
    -> (u32, u64, u64, u32, u32, u32, u64) {
""" + timing + "\n}\n"
    inputs = {"lib.rs": args.source.resolve(),
              "checks.rs": ROOT / "scripts/coreaudio_timing_tests.rs",
              "perf.rs": ROOT / "tests/support/perf.rs",
              "kernel_bench.rs": ROOT / "tests/support/kernel_bench.rs"}
    hashes = {}
    for name, path in inputs.items():
        data = path.read_bytes()
        (destination / name).write_bytes(data)
        hashes[name] = hashlib.sha256(data).hexdigest()
    generated += '\n#[path = "perf.rs"] mod perf;\n'
    generated += '#[path = "kernel_bench.rs"] mod kernel_bench;\n'
    generated += '#[path = "checks.rs"] mod checks;\n'
    (destination / "generated.rs").write_text(generated, encoding="utf-8")
    executable = destination / ("timing.exe" if platform.system() == "Windows" else "timing")
    command = ["rustc", "--edition=2024", "--test", str(destination / "generated.rs"),
               "-o", str(executable)]
    if not args.debug:
        command += ["-C", "opt-level=3", "-C", "lto=thin", "-C", "codegen-units=1"]
    if platform.system() == "Windows":
        command += ["-C", "target-feature=+crt-static"]
    subprocess.run(command, check=True, cwd=ROOT)
    result = subprocess.run([str(executable), "--test-threads=1"], check=True,
                            capture_output=True, text=True)
    (destination / "checks.log").write_text(result.stdout, encoding="utf-8")
    print(result.stdout)
    metadata = {"inputs_sha256": hashes, "command": command, "operations_per_batch": 256,
                "executable_sha256": hashlib.sha256(executable.read_bytes()).hexdigest(),
                "rustc": subprocess.check_output(["rustc", "-Vv"], text=True),
                "head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                "platform": platform.platform()}
    (destination / "manifest.json").write_text(json.dumps(metadata, indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()
