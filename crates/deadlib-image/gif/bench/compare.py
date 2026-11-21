"""Compare successive optimization runs without dropping individual regressions."""
import argparse
import json
import math
from pathlib import Path

from summarize import FIRST_MODES, MODES, corpus, read


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before", type=Path)
    parser.add_argument("after", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--first-frame", action="store_true")
    args = parser.parse_args()
    modes = FIRST_MODES if args.first_frame else MODES
    before, after = read(args.before, modes), read(args.after, modes)
    assert before.keys() == after.keys(), "Mismatched measured input sets"
    groups = {"all": list(before)}
    for file in before:
        groups.setdefault(corpus(file), []).append(file)
    result = {}
    for group, files in groups.items():
        modes = {}
        for mode in sorted(next(iter(before.values()))):
            metrics = {}
            for metric in ("thread_cycles", "ns", "alloc_calls", "requested_bytes"):
                a = [before[file][mode][metric] for file in files]
                b = [after[file][mode][metric] for file in files]
                if not any(a + b):
                    continue
                assert all(n > 0 for n in a + b), (group, mode, metric)
                metrics[metric] = {
                    "after_before_geomean": math.exp(
                        sum(math.log(y / x) for x, y in zip(a, b)) / len(files)
                    ),
                    "before_total": sum(a),
                    "after_total": sum(b),
                    "improved": sum(y < x for x, y in zip(a, b)),
                    "ties": sum(y == x for x, y in zip(a, b)),
                    "regressions": [
                        {"file": file, "before": x, "after": y, "ratio": y / x}
                        for file, x, y in zip(files, a, b) if y > x
                    ],
                }
            modes[mode] = metrics
        result[group] = {"files": len(files), "modes": modes}
        print(group, len(files))
        for mode, metrics in modes.items():
            print(" ", mode, ", ".join(
                f"{metric}={data['after_before_geomean']:.4f}"
                for metric, data in metrics.items()
            ))
    args.output.write_text(json.dumps(result, indent=2) + "\n")


if __name__ == "__main__":
    main()
