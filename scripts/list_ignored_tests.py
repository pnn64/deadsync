"""List source ignore attributes, with optional expanded names from a Cargo build.

python scripts/list_ignored_tests.py --output docs/testing/ignored-tests.md
Add --artifacts FILE for cargo test --no-run --message-format=json output.
"""
from pathlib import Path
from collections import Counter
import argparse
import json
import re
import subprocess


def category(reason):
    text = reason.lower()
    if any(word in text for word in ["benchmark", "performance", "timing"]):
        return "benchmark/timing"
    if (any(word in text for word in ["song", "corpus", "cuphead", "mawaru", "7th gear"])
            or ("compiles" in text and "semantic check" in text)):
        return "long song fixture"
    if any(word in text for word in ["gpu", "device", "window", "opengl", "vulkan", "directx", "audio endpoint"]):
        return "hardware/display"
    return "other"


def execution_status(rows):
    latest = {row.get("backend"): row for row in rows}
    if any(backend is not None for backend in latest):
        latest.pop(None, None)  # Explicit backend runs supersede an unconfigured probe.
    records = list(latest.values())
    if not records:
        return "not recorded"
    if any(row.get("code") not in (None, 0) for row in records):
        return "failed"
    if all(row.get("deferred") for row in records):
        return "deferred: long song fixture"
    if not all(row.get("code") == 0 for row in records):
        return "not recorded"
    if any(row.get("reused_identical_artifact") for row in records):
        return "passed: identical current benchmark artifact (prior execution)"
    if all(row.get("verification") == "executed current artifact" for row in records):
        status = "passed: current artifact"
    else:
        status = "passed: prior audit"
    backends = sorted(backend for backend in latest if backend is not None)
    return status + (" (" + ", ".join(backends) + ")" if backends else "")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--artifacts", type=Path)
    parser.add_argument("--executions", type=Path, help="JSON list of explicit ignored-test results")
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    files = subprocess.check_output(
        ["git", "ls-files", "--cached", "--others", "--exclude-standard", "--", "*.rs"],
        cwd=root, text=True, encoding="utf-8").splitlines()
    rows = []
    for name in sorted(set(files)):
        path = root / name
        if not path.is_file():
            continue
        lines = path.read_text(encoding="utf-8-sig").splitlines()
        for index, line in enumerate(lines):
            match = re.match(r'\s*#\[ignore(?:\s*=\s*"([^"]*)")?\]', line)
            if match is None:
                continue
            function = re.search(r'\bfn\s+([^\s(]+)', "\n".join(lines[index + 1:index + 20]))
            reason = match[1] or "(no reason recorded)"
            rows.append((function[1] if function else "<macro>", name, index + 1, reason))
    counts = Counter(category(row[3]) for row in rows)
    result = ["# Ignored test inventory", "",
        f"Current source inventory: **{len(rows)} ignore attributes**. These are source attributes, not macro-expanded test counts. The initial audit had 249.", "",
        "Six entries in vendored rfd are Linux dialog checks; target platform configuration determines which tests exist in a Windows build.", "",
        ("Only long song-fixture runs are authorized deferrals for this cleanup. The expanded table records explicit passes and permitted long-song deferrals."
         if args.executions else
         "Only long song-fixture runs are authorized deferrals for this cleanup. Benchmark and hardware ignores require explicit execution; listing alone does not declare them passing."), "",
        "| Category | Source attributes |", "| --- | ---: |"]
    result += [f"| {label} | {number} |" for label, number in sorted(counts.items())]
    result += ["", "| Test or macro | Source | Recorded reason |", "| --- | --- | --- |"]
    for function, name, line, reason in rows:
        reason = reason.replace("|", "\\|")
        result.append(f"| `{function}` | [{name}:{line}](../../{name}) | {reason} |")
    if args.artifacts:
        targets = {}
        for line in args.artifacts.read_text(encoding="utf-8").splitlines():
            try:
                item = json.loads(line)
            except json.JSONDecodeError:
                continue
            if item.get("reason") == "compiler-artifact" and item.get("executable") and item.get("profile", {}).get("test"):
                if item["target"]["name"] in ["full_song_lua", "live_song_lua"]:
                    continue
                key = (item["package_id"], item["target"]["name"], tuple(item["target"]["kind"]))
                targets[key] = item
        expanded = []
        for item in targets.values():
            output = subprocess.check_output([item["executable"], "--list", "--ignored", "--format=terse"],
                cwd=root, text=True, encoding="utf-8")
            expanded += [(item["target"]["name"], name) for name in re.findall(r"^(.*): test$", output, re.M)]
        executions = {}
        if args.executions:
            for row in json.loads(args.executions.read_text(encoding="utf-8")):
                executions.setdefault((row["target"], row["name"]), []).append(row)
        result += ["", f"## Expanded names in this build ({len(expanded)})", "",
            "From the supplied Cargo artifact receipt. CLI harnesses are excluded from libtest enumeration.", "",
            "| Target | Ignored test | Explicit audit |", "| --- | --- | --- |"]
        for target, name in sorted(expanded):
            status = execution_status(executions.get((target, name), []))
            result.append(f"| `{target}` | `{name}` | {status} |")
    text = "\n".join(result) + "\n"
    if args.output:
        args.output.write_text(text, encoding="utf-8")
    else:
        print(text, end="")


if __name__ == "__main__":
    main()
