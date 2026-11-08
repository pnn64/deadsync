"""Exact C checks of native coefficient encodes from official T.83 models."""
import argparse
import hashlib
import itertools
import json
from pathlib import Path
import shutil
import subprocess
from stream_support import update_tables, prefix_tables


def sha(data):
    return hashlib.sha256(data).hexdigest()


def run(command):
    result = subprocess.run([str(v) for v in command], capture_output=True, timeout=120)
    assert result.returncode == 0 and not result.stderr, (command, result.returncode, result.stderr.decode(errors="replace"))


def main():
    parser = argparse.ArgumentParser()
    for name in ("rewrite", "ijg", "turbo", "moz8", "moz12", "decoded", "out", "fixtures"):
        parser.add_argument("--" + name, required=True, type=Path)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    tools = args.out / "tools"
    tools.mkdir(exist_ok=True)
    hashes = {}
    for name in ("rewrite", "ijg", "turbo", "moz8", "moz12"):
        source = getattr(args, name)
        target = tools / (name + source.suffix)
        shutil.copy2(source, target)
        hashes[name] = sha(target.read_bytes())
        setattr(args, name, target.resolve())
    original = json.loads((args.decoded / "report.json").read_text())
    bank = {}
    update_tables(bank, (args.fixtures / "B1.JPG").read_bytes(), True)
    records = []
    for source in original["results"]:
        if source["kind"] != "DCT":
            continue
        image = args.fixtures / source["file"]
        assert sha(image.read_bytes()) == source["sha256"]
        expected = (args.decoded / (image.stem + ".image-0.coef")).read_bytes()
        assert sha(expected) == source["coefficient_sha256"]
        source_copy = False
        if source["components"] == 255:
            # The rewrite CLI accepts interchange input. The original pair was
            # independently checked through real B.5 APIs by reference_t83.py.
            wire = prefix_tables(bank, image.read_bytes())
            image = args.out / "B2-equivalent-interchange.jpg"
            image.write_bytes(wire)
            source_copy = True
        for arithmetic, progressive, optimized, restart in itertools.product((0, 1), (0, 1), (0, 1), (0, 7)):
            if arithmetic and not optimized:
                continue
            if progressive and source["components"] > 4:
                continue
            target = args.out / "native.jpg"
            run([args.rewrite, image, target, progressive, optimized, restart, arithmetic])
            ijg = args.out / "ijg.coef"
            run([args.ijg, target, ijg])
            assert ijg.read_bytes() == expected, (source["file"], "IJG")
            row = {"source": source["file"], "source_sha256": source["sha256"],
                   "B2_equivalent_interchange_source": source_copy,
                   "arithmetic": arithmetic, "progressive": progressive,
                   "optimized_Huffman": bool(optimized and not arithmetic), "restart_interval": restart,
                   "bytes": target.stat().st_size, "sha256": sha(target.read_bytes()), "IJG": "exact"}
            if source["components"] <= 4:
                prefix = args.out / "turbo"
                run([args.turbo, target, prefix])
                assert prefix.with_suffix(".coef").read_bytes() == expected
                row["Turbo"] = "exact"
                if arithmetic and source["precision"] == 12:
                    row["mozjpeg"] = "skipped: upstream 12-bit arithmetic exclusion"
                else:
                    prefix = args.out / "mozjpeg"
                    run([args.moz8 if source["precision"] == 8 else args.moz12, target, prefix])
                    assert prefix.with_suffix(".coef").read_bytes() == expected
                    row["mozjpeg"] = "exact"
            else:
                row["Turbo"] = "skipped: selector lookup beyond first four components"
                row["mozjpeg"] = "skipped: default component-count cap"
            records.append(row)
    assert len(records) == 186
    counts = {name: sum(row.get(name) == "exact" for row in records) for name in ("IJG", "Turbo", "mozjpeg")}
    assert counts == {"IJG": 186, "Turbo": 180, "mozjpeg": 156}
    report = {"scope": "Native coefficient writer on official T.83 decoded models; exact C observations, not source-pixel/FDCT encoder comparisons.",
              "tools": hashes, "driver_sha256": sha(Path(__file__).read_bytes()),
              "original_decode_report_sha256": sha((args.decoded / "report.json").read_bytes()),
              "cases": len(records), "exact_C_comparisons": counts, "results": records}
    (args.out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({k: v for k, v in report.items() if k not in ("results", "tools")}), flush=True)


if __name__ == "__main__":
    main()
