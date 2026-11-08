"""Execute the non-song ignored audit against current debug/release artifacts.

The prior audit supplies explicit long-song deferrals and execution profiles.
Each test gets a fresh process. A small set of unchanged, generated-input
benchmarks may reuse a passing result only for the identical current executable.
Hardware checks and changed executables always run.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import time


# These workloads construct their buffers/geometry in code. They do not consume
# the oracle captures refreshed by this cleanup. The exact executable includes
# their code and embedded inputs; matching it preserves the prior observation.
GENERATED_BENCHMARK_TARGETS = {
    "deadlib_render_backend_software", "batching_pixels", "stripe_bins",
    "deadlib_audio_core", "deadlib_audio_decode",
}
GPU_CAPTURE_CASE = (
    "deadsync_shell", "gameplay_regression_tests::tests::sprite_core_gpu_capture_writes_baseline",
)
GPU_CAPTURE_BACKENDS = {"vulkan", "opengl", "vulkan-wgpu"}


def classified_previous(rows, current):
    prior = {}
    for row in rows:
        case = (row["target"], row["name"])
        if case not in current:
            continue
        backend = row.get("backend")
        if case == GPU_CAPTURE_CASE:
            # The unconfigured probe failed before the required backend runs.
            # Those explicit runs supersede it; all three remain mandatory.
            if backend is None:
                continue
            assert backend in GPU_CAPTURE_BACKENDS, f"unknown capture backend: {backend}"
        prior[(*case, backend)] = row
    if GPU_CAPTURE_CASE in current:
        assert {backend for target, name, backend in prior
                if (target, name) == GPU_CAPTURE_CASE} == GPU_CAPTURE_BACKENDS, "missing GPU backend classification"
    assert {(target, name) for target, name, _ in prior} == current, "new ignored cases require classification"
    return prior


def artifacts(path):
    found = {}
    complete = False
    for line in path.read_text(encoding="utf-8").splitlines():
        value = json.loads(line)
        if value.get("reason") == "build-finished":
            complete = value.get("success") is True
        if (value.get("reason") == "compiler-artifact" and value.get("executable")
                and value.get("profile", {}).get("test")):
            found[(value["package_id"], value["target"]["name"], tuple(value["target"]["kind"]))] = value["executable"]
    assert complete, f"artifact build has not completed successfully: {path}"
    return [(key[1], executable) for key, executable in found.items()]


def ignored_cases(build_artifacts, root):
    cases = {}
    for target, executable in build_artifacts:
        if target in ["full_song_lua", "live_song_lua"]:
            continue
        result = subprocess.check_output([executable, "--list", "--ignored", "--format=terse"],
                                         cwd=root, text=True, encoding="utf-8")
        for name in re.findall(r"^(.*): test$", result, re.M):
            key = (target, name)
            assert key not in cases or cases[key] == executable, f"ambiguous target/name: {key}"
            cases[key] = executable
    return cases


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--debug-artifacts", required=True, type=Path)
    parser.add_argument("--release-artifacts", required=True, type=Path)
    parser.add_argument("--prior", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    profiles = {"debug": ignored_cases(artifacts(args.debug_artifacts), root),
                "release": ignored_cases(artifacts(args.release_artifacts), root)}
    current = set(profiles["debug"])
    prior = classified_previous(json.loads(args.prior.read_bytes()), current)
    for (target, name, _), previous in prior.items():
        if not previous.get("deferred"):
            mode = previous.get("mode", "release" if "release" in Path(previous["exe"]).parts else "debug")
            assert (target, name) in profiles[mode], f"missing current {mode} artifact: {target}::{name}"
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    rows, hashes = [], {}
    record = output / "executions.json"
    for (target, name, backend), previous in sorted(prior.items(), key=lambda item: str(item[0])):
        if previous.get("deferred"):
            rows.append({**previous, "verification": "permitted long-song deferral"})
            record.write_bytes((json.dumps(rows, indent=2) + "\n").encode())
            continue
        assert previous.get("code") == 0, f"unresolved prior failure: {target}::{name}"
        mode = previous.get("mode", "release" if "release" in Path(previous["exe"]).parts else "debug")
        executable = profiles[mode][(target, name)]
        if executable not in hashes:
            hashes[executable] = hashlib.sha256(Path(executable).read_bytes()).hexdigest()
        binary_hash = hashes[executable]
        reuse = (target in GENERATED_BENCHMARK_TARGETS and "benchmark" in name
                 and binary_hash == previous.get("sha256"))
        prior_log_bytes = None
        if reuse and Path(previous.get("log", "")).is_file():
            candidate = Path(previous["log"]).read_bytes()
            text = candidate.decode("utf-8", errors="replace")
            if name in text and re.search(r"test result: ok\. 1 passed; 0 failed;", text):
                prior_log_bytes = candidate
        if reuse and prior_log_bytes is not None:
            rows.append({**previous, "exe": executable, "sha256": binary_hash,
                "reused_identical_artifact": True,
                "reused_prior_log_sha256": hashlib.sha256(prior_log_bytes).hexdigest(),
                "verification": "identical freshly built executable; generated/embedded benchmark inputs unchanged"})
        else:
            slug = re.sub(r"[^a-zA-Z0-9_-]", "_", target + "--" + name + ("--" + backend if backend else ""))
            log = output / (slug + ".log")
            env = os.environ.copy()
            if backend:
                env["DEADSYNC_F0_BACKEND"] = backend
            started = time.monotonic()
            with log.open("wb") as stream:
                result = subprocess.run([executable, name, "--exact", "--ignored", "--nocapture", "--test-threads=1"],
                                        cwd=root, env=env, stdout=stream, stderr=subprocess.STDOUT)
            text = log.read_text(encoding="utf-8", errors="replace")
            code = result.returncode
            if code == 0 and not re.search(r"test result: ok\. 1 passed; 0 failed;", text):
                code = 1  # A successful filter matching no tests is not a pass.
            rows.append({"target": target, "name": name, "backend": backend, "exe": executable,
                "mode": mode, "sha256": binary_hash, "code": code, "seconds": time.monotonic() - started,
                "log": str(log), "verification": "executed current artifact"})
            print(f'{target}::{name}: {code}, {rows[-1]["seconds"]:.2f}s', flush=True)
        record.write_bytes((json.dumps(rows, indent=2) + "\n").encode())
    failures = [row for row in rows if row.get("code", 0) != 0]
    summary = {"cases": len(current), "passed_cases": len({(row["target"], row["name"]) for row in rows if row.get("code") == 0}),
        "deferred_cases": sum(bool(row.get("deferred")) for row in rows),
        "reused_identical_artifacts": sum(bool(row.get("reused_identical_artifact")) for row in rows),
        "failures": failures}
    (output / "summary.json").write_bytes((json.dumps(summary, indent=2) + "\n").encode())
    print(json.dumps({key: value for key, value in summary.items() if key != "failures"}))
    raise SystemExit(bool(failures))


if __name__ == "__main__":
    main()
