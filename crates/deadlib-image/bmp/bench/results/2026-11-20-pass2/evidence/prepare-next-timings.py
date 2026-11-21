from pathlib import Path
import hashlib
import json

root = Path.cwd()
task = root / "target/bmp-decode-opt-20261120-pass2"
source = task / "watch_commands.py"
watcher = source.read_text(encoding="utf-8")
old = "COMPILERS = {'cargo.exe', 'rustc.exe', 'rustdoc.exe', 'cl.exe', 'link.exe', 'lld-link.exe'}"
new = "COMPILERS = {'cargo.exe', 'rustc.exe', 'rustdoc.exe', 'clippy-driver.exe', 'cl.exe', 'clang.exe', 'clang-cl.exe', 'link.exe', 'lld.exe', 'lld-link.exe', 'ninja.exe', 'cmake.exe'}"
assert watcher.count(old) == 1
watcher = watcher.replace(old, new)
old = "            'pump_modes', 'genre_metadata', 'deadlib_image')"
new = "            'pump_modes', 'genre_metadata', 'deadlib_image',\n            'bench', 'timing', 'snapshot', 'rssp')"
assert watcher.count(old) == 1
watcher = watcher.replace(old, new)
strict = task / "watch_commands-strict.py"
with strict.open("x", encoding="utf-8", newline="\n") as output:
    output.write(watcher)

identities = {}
for stage in ("baseline", "candidate2-rgba4444", "candidate3-direct-rgba"):
    records = json.loads((task / stage / "executables.json").read_text(encoding="utf-8-sig"))
    expected = next(row["sha256"] for row in records if row["path"] == "bmp-timing.exe")
    actual = hashlib.sha256((task / stage / "bmp-timing.exe").read_bytes()).hexdigest()
    assert actual == expected
    identities[stage] = actual


def command(stage, label, mode, milliseconds, layout=None, corpus=False):
    args = ["powershell.exe", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File",
            "target/bmp-decode-opt-20261120-pass2/measure.ps1",
            "-Executable", f"target/bmp-decode-opt-20261120-pass2/{stage}/bmp-timing.exe",
            "-Label", label, "-Mode", mode, "-Milliseconds", str(milliseconds)]
    if layout:
        args += ["-Filter", layout]
    if corpus:
        args += ["-Corpus"]
    return {"name": label, "command": args, "cwd": str(root)}


focus = []
for layout in ("32", "rgb333", "rgb555", "rgb565"):
    for stage, when, mode, short in (
        ("candidate2-rgba4444", "before", "decode", "decode"),
        ("candidate3-direct-rgba", "after", "decode", "decode"),
        ("candidate3-direct-rgba", "after", "owned-rgba", "owned"),
        ("candidate2-rgba4444", "before", "owned-rgba", "owned"),
    ):
        label = f"candidate3-focus-{when}-{short}-{layout}"
        focus.append(command(stage, label, mode, 40, layout=layout))

full = []
for mode, short, order, corpus in (
    ("decode", "decode", (("baseline", "before"), ("candidate3-direct-rgba", "after")), False),
    ("owned-rgba", "owned", (("candidate3-direct-rgba", "after"), ("baseline", "before")), False),
    ("rgba", "rgba", (("baseline", "before"), ("candidate3-direct-rgba", "after")), False),
    ("owned-rgba", "corpus", (("candidate3-direct-rgba", "after"), ("baseline", "before")), True),
):
    for stage, when in order:
        label = f"baseline-final-matrix-{when}-{short}"
        full.append(command(stage, label, mode, 20, corpus=corpus))

for name, jobs in (("candidate3-focus-commands.json", focus), ("baseline-final-matrix-commands.json", full)):
    with (task / name).open("x", encoding="utf-8", newline="\n") as output:
        json.dump(jobs, output, indent=2)
        output.write("\n")
record = {
    "status": "prepared, not executed; exclusive timing grant required",
    "watcher_parent_sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
    "watcher_sha256": hashlib.sha256(strict.read_bytes()).hexdigest(),
    "watcher_change": "also detects all bench/timing/snapshot/rssp names and clang/clippy/ninja/cmake processes; matching external process aborts the current descendant tree",
    "frozen_timing_executables": identities,
    "focus": {"jobs": len(focus), "batches": 5, "milliseconds": 40, "cpu": 2, "native_cases": 64, "decode_rows_per_binary": 64, "owned_rows_per_binary": 106},
    "full_matrix": {"jobs": len(full), "batches": 5, "milliseconds": 20, "cpu": 2, "decode_rows": 120, "owned_rows": 218, "rgba_rows": 218, "corpus_rows": 24},
    "conditions": [
        "Focus compares candidate3 directly to its frozen candidate2 parent.",
        "Full matrix explicitly compares the original frozen baseline with candidate3, not with a descendant baseline.",
        "Full matrix must be regenerated with a new label/executable identity if candidate3 is rejected.",
        "All per-input losses and native/image semantic differences remain in the raw outputs.",
        "RGB333, RGB555, RGB565 controls are explicit in focus and full matrix.",
    ],
}
with (task / "next-timing-plan.json").open("x", encoding="utf-8", newline="\n") as output:
    json.dump(record, output, indent=2)
    output.write("\n")
print(json.dumps(record, indent=2))

