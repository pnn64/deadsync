"""Archive CoreAudio setup functions with deterministic property/log fixtures.

Only the operating-system queries and log sink are replaced. CPU results measure
one fixture call, including the returned range vector and formatting when enabled.
"""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import subprocess

from benchmark_coreaudio import ROOT, SOURCE, function


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", type=Path, default=SOURCE)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--debug", action="store_true")
    args = parser.parse_args()
    folder = args.archive.resolve()
    folder.mkdir(parents=True, exist_ok=True)
    source = args.source.read_text(encoding="utf-8")
    constant = source[source.index("const COMMON_SAMPLE_RATES_HZ:"):]
    constant = constant[:constant.index("];") + 2]
    generated = constant + "\n"
    for name in ["supported_sample_rates_hz", "push_sample_rate_hz", "nanos_to_ms"]:
        generated += function(source, name).removeprefix("#[inline(always)]\n") + "\n"
    start = source[source.index("pub fn start("):source.index("impl OutputChangeListener")]
    declaration = next((line for line in start.splitlines()
                        if "let device_name =" in line), "")
    info = start[start.index("    info!("):start.index("    Ok(CoreAudioOutputStream")]
    generated += """
fn startup_log(prep: &Prep) {
    let sample_rate_hz = prep.sample_rate_hz;
    let buffer_frames = prep.buffer_frames;
    let output_latency_ns = prep.latency.nanos;
""" + declaration + "\n" + info + "}\n"
    refresh = source[source.index("        let event = (device_id != previous_device_id)"):]
    refresh = refresh[:refresh.index("\n    }\n}")]
    generated += """
fn refresh_log(device_id: u32, previous_device_id: u32, current: OutputLatency,
               previous_ns: u64, previous_frames: u32, previous_rate_hz: u32) {
""" + refresh + "\n}\n"
    inputs = {"lib.rs": args.source.resolve(),
              "fixtures.rs": ROOT / "scripts/coreaudio_setup_tests.rs",
              "perf.rs": ROOT / "tests/support/perf.rs",
              "kernel_bench.rs": ROOT / "tests/support/kernel_bench.rs"}
    hashes = {}
    for name, path in inputs.items():
        data = path.read_bytes()
        (folder / name).write_bytes(data)
        hashes[name] = hashlib.sha256(data).hexdigest()
    generated = 'include!("fixtures.rs");\n' + generated
    generated += '#[path = "perf.rs"] mod perf;\n'
    generated += '#[path = "kernel_bench.rs"] mod kernel_bench;\n'
    (folder / "generated.rs").write_text(generated, encoding="utf-8")
    executable = folder / ("coreaudio.exe" if platform.system() == "Windows" else "coreaudio")
    command = ["rustc", "--edition=2024", "--test", str(folder / "generated.rs"), "-o", str(executable)]
    if not args.debug:
        command += ["-C", "opt-level=3", "-C", "lto=thin", "-C", "codegen-units=1"]
    if platform.system() == "Windows":
        command += ["-C", "target-feature=+crt-static"]
    subprocess.run(command, check=True, cwd=ROOT)
    checks = subprocess.check_output([str(executable), "--test-threads=1"], text=True)
    (folder / "checks.log").write_text(checks, encoding="utf-8")
    print(checks)
    manifest = {"inputs_sha256": hashes, "command": command,
                "executable_sha256": hashlib.sha256(executable.read_bytes()).hexdigest(),
                "generated_sha256": hashlib.sha256((folder / "generated.rs").read_bytes()).hexdigest(),
                "rustc": subprocess.check_output(["rustc", "-Vv"], text=True),
                "head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
                "platform": platform.platform(), "operations_per_batch": 1}
    (folder / "manifest.json").write_text(json.dumps(manifest, indent=2), encoding="utf-8")


if __name__ == "__main__":
    main()
