"""Three-C-oracle coefficient gate and mozjpeg float-IDCT/color comparisons.

Reference binaries remain test-only. Inputs are either authored by mozjpeg,
native coefficient rewrites, or earlier retained regressions. This is not an
independent standards-conformance corpus or a performance benchmark.
"""
import argparse
import hashlib
import itertools
import json
from pathlib import Path
import shutil
import struct
import subprocess
from stream_support import marker_ranges


def sha(data):
    return hashlib.sha256(data).hexdigest()


def run(command):
    result = subprocess.run([str(v) for v in command], capture_output=True, timeout=90)
    assert result.returncode == 0 and not result.stderr, (command, result.returncode, result.stderr.decode(errors="replace"))
    return result


def samples(precision, pattern):
    width, height = ((1, 1), (17, 19), (47, 33))[pattern]
    maximum = (1 << precision) - 1
    values = []
    for y in range(height):
        for x in range(width):
            for c in range(3):
                v = ((c + 1) * maximum // 4 if pattern == 0 else
                     maximum * ((x // 3 + y // 3 + c) % 2) if pattern == 1 else
                     (x * 193 + y * 317 + c * 911 + x * y * 53) & maximum)
                values.append(v)
    payload = bytes(values) if precision == 8 else struct.pack(">" + "H" * len(values), *values)
    return f"P6\n{width} {height}\n{maximum}\n".encode() + payload


def info(wire):
    for marker, start, _ in marker_ranges(wire):
        if marker in (0xc0, 0xc1, 0xc2, 0xc9, 0xca):
            return wire[start + 4], marker
    raise AssertionError("ordinary DCT SOF required")


def words(data):
    assert len(data) % 2 == 0
    return struct.unpack("<" + "H" * (len(data) // 2), data)


def error(a, b):
    assert len(a) == len(b) and a
    errors = [abs(x - y) for x, y in zip(a, b)]
    return {"max": max(errors), "mean": sum(errors) / len(errors), "samples": len(errors)}


def main():
    parser = argparse.ArgumentParser()
    for name in ("build", "native", "colors", "rewrite", "turbo", "ijg", "out"):
        parser.add_argument("--" + name, required=True, type=Path)
    parser.add_argument("--fixtures", type=Path)
    parser.add_argument("--resume", action="store_true")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    work = args.out / "case"
    work.mkdir(exist_ok=True)
    tools = args.out / "tools"
    tools.mkdir(exist_ok=True)
    build = json.loads(args.build.read_text())
    paths = {name: getattr(args, name) for name in ("native", "colors", "rewrite", "turbo", "ijg")}
    for row in build["builds"]:
        for name, record in row["tools"].items():
            path = Path(record["path"])
            assert sha(path.read_bytes()) == record["sha256"]
            paths[f"moz{row['precision']}_{name}"] = path
    hashes = {}
    for name, path in paths.items():
        target = tools / (name + path.suffix)
        if not args.resume:
            shutil.copy2(path, target)
        hashes[name] = sha(target.read_bytes())
        assert hashes[name] == sha(path.read_bytes())
        paths[name] = target.resolve()
    identity = {"tools": hashes, "driver_sha256": sha(Path(__file__).read_bytes()),
                "build_report_sha256": sha(args.build.read_bytes())}
    checkpoint = args.out / "checkpoint.jsonl"
    if args.resume:
        rows = [json.loads(v) for v in checkpoint.read_text().splitlines()]
        assert rows[0] == identity
        done = {v["case"]: v for v in rows[1:]}
    else:
        checkpoint.write_text(json.dumps(identity) + "\n")
        done = {}
    records, retained = [], []
    if args.fixtures:
        args.fixtures.mkdir(parents=True, exist_ok=True)

    def check(name, image, origin, expected=None, pin=False):
        if name in done:
            assert done[name]["sha256"] == sha(image.read_bytes()), name
            records.append(done[name])
            return
        precision, process = info(image.read_bytes())
        assert not (precision == 12 and process in (0xc9, 0xca))
        prefixes = {key: work / key for key in ("moz", "turbo", "native", "rgb")}
        for prefix in prefixes.values():
            for suffix in (".coef", ".planes", ".color", ".rgb", ".pixels"):
                prefix.with_suffix(suffix).unlink(missing_ok=True)
        run([paths[f"moz{precision}_oracle"], image, prefixes["moz"]])
        run([paths["turbo"], image, prefixes["turbo"]])
        run([paths["native"], image, prefixes["native"]])
        ijg = work / "ijg.coef"
        run([paths["ijg"], image, ijg])
        coefficients = prefixes["moz"].with_suffix(".coef").read_bytes()
        for key in ("turbo", "native"):
            assert prefixes[key].with_suffix(".coef").read_bytes() == coefficients, (name, key)
        assert ijg.read_bytes() == coefficients, (name, "ijg")
        if expected is not None:
            assert coefficients == expected, (name, "pre-rewrite coefficients")
        native_planes = words(prefixes["native"].with_suffix(".planes").read_bytes())
        moz_planes = words(prefixes["moz"].with_suffix(".planes").read_bytes())
        raw = error(native_planes, moz_planes)
        raw_checked = not name.startswith("replay-arithmetic-model-")
        if raw_checked:
            assert raw["max"] <= 1, (name, "float IDCT", raw)
        # Record C/C numerical differences rather than assuming shared ancestry
        # makes their floating point outputs identical.
        c_raw = error(words(prefixes["turbo"].with_suffix(".planes").read_bytes()), moz_planes)
        record = {"case": name, "origin": origin, "precision": precision, "process": process,
                  "sha256": sha(image.read_bytes()), "bytes": image.stat().st_size,
                  "coefficient_sha256": sha(coefficients), "three_C_coefficients": "exact",
                  "native_float_IDCT": raw, "raw_IDCT_checked": raw_checked,
                  "Turbo_mozjpeg_float_IDCT": c_raw}
        if not raw_checked:
            record["raw_IDCT_exclusion"] = "Synthetic full-magnitude entropy-model fixture, not bounded source FDCT; C float range limiting is not a pixel oracle. Coefficients remain exact."
        if raw_checked and prefixes["moz"].with_suffix(".color").exists():
            # This matrix uses gray/RGB/YCbCr; CMYK is replayed separately below.
            count = struct.unpack_from("<I", coefficients, 16)[0]
            run([paths["colors"], image, prefixes["rgb"], "nearest"])
            suffix = ".cmyk" if count == 4 else ".rgb"
            color = error(words(prefixes["rgb"].with_suffix(suffix).read_bytes()),
                          words(prefixes["moz"].with_suffix(".color").read_bytes()))
            tolerance = 4 if count in (3, 4) else 1
            assert color["max"] <= tolerance, (name, "nearest color", color)
            record["native_nearest_color"] = color
            record["color_tolerance"] = tolerance
        if pin and args.fixtures:
            artifacts = []
            for suffix, data in ((".jpg", image.read_bytes()), (".coef", coefficients),
                                 (".planes", prefixes["moz"].with_suffix(".planes").read_bytes())):
                target = args.fixtures / (name + suffix)
                target.write_bytes(data)
                artifacts.append({"file": target.name, "sha256": sha(data)})
            record["retained"] = artifacts
        records.append(record)
        with checkpoint.open("a") as f:
            f.write(json.dumps(record) + "\n")
        if len(records) % 50 == 0:
            print(json.dumps({"checked": len(records), "last": name}), flush=True)

    layouts = [("gray", ["-grayscale"]), ("rgb", ["-rgb"])] + [
        (name, ["-sample", sampling]) for name, sampling in (
            ("444", "1x1,1x1,1x1"), ("422", "2x1,1x1,1x1"),
            ("420", "2x2,1x1,1x1"), ("411", "4x1,1x1,1x1"),
            ("440", "1x2,1x1,1x1"), ("410", "4x2,1x1,1x1"),
            ("3x2", "3x2,1x1,1x1"))]
    image, source = work / "image.jpg", work / "source.ppm"
    for precision, arithmetic in ((8, 0), (8, 1), (12, 0)):
        for progressive, (layout, options), quality, pattern in itertools.product((0, 1), layouts, (20, 83, 100), range(3)):
            name = f"moz-p{precision}-a{arithmetic}-v{progressive}-{layout}-q{quality}-s{pattern}"
            source.write_bytes(samples(precision, pattern))
            command = [paths[f"moz{precision}_cjpeg"], "-quality", quality,
                       "-progressive" if progressive else "-baseline", *options,
                       *(["-fastcrush"] if progressive and pattern else []),
                       "-restart", (0, 1, 7)[pattern], *(["-arithmetic"] if arithmetic else []),
                       "-outfile", image, source]
            run(command)
            pin = pattern == 1 and quality == 83 and layout in ("gray", "420")
            check(name, image, {"kind": "mozjpeg source", "options": [str(v) for v in command[1:-3]]}, pin=pin)
            if pattern == 1 and quality == 83 and layout in ("gray", "rgb", "420"):
                original = work / "original.jpg"
                original.write_bytes(image.read_bytes())
                original_coefficients = work / "original.coef"
                run([paths["ijg"], original, original_coefficients])
                expected = original_coefficients.read_bytes()
                for v, a, optimize, restart in ((0, 0, 0, 0), (0, 0, 1, 7), (1, 0, 1, 1), (0, 1, 1, 1), (1, 1, 1, 7)):
                    if precision == 12 and a:
                        continue
                    rewrite = work / "rewrite.jpg"
                    run([paths["rewrite"], original, rewrite, v, optimize, restart, a])
                    check(name + f"-rewrite-v{v}-a{a}-o{optimize}-r{restart}", rewrite,
                          {"kind": "native coefficient rewrite", "source": sha(original.read_bytes())}, expected)

    # Exercise scan optimization on nontrivial sources without restarts. The
    # restart/scan-optimizer producer defect has a separate strict rejection gate.
    for precision, arithmetic in ((8, 0), (8, 1), (12, 0)):
        source.write_bytes(samples(precision, 2))
        for (layout, options), quality in itertools.product(layouts, (20, 83, 100)):
            name = f"moz-p{precision}-a{arithmetic}-optimized-{layout}-q{quality}"
            run([paths[f"moz{precision}_cjpeg"], "-quality", quality, *options,
                 *(["-arithmetic"] if arithmetic else []), "-outfile", image, source])
            check(name, image, {"kind": "mozjpeg default optimized scans without restarts"},
                  pin=quality == 83 and layout in ("gray", "420"))

    # Explicit scripts and encoder-specific quantization/trellis settings.
    for precision, arithmetic in ((8, 0), (8, 1), (12, 0)):
        source.write_bytes(samples(precision, 2))
        styles = [("separate", ["-scans", work / "separate.scans"]),
                  ("refine", ["-scans", work / "refine.scans", "-fastcrush"])]
        (work / "separate.scans").write_text("2; 0; 1;\n")
        (work / "refine.scans").write_text(
            "0,1,2: 0,0,0,0; 2: 1,63,0,2; 1: 1,63,0,2; 0: 1,63,0,2;\n" +
            "".join(f"2: 1,63,{ah},{ah-1}; 0: 1,63,{ah},{ah-1}; 1: 1,63,{ah},{ah-1};\n" for ah in (2, 1)))
        styles += [("quant" + str(q), ["-quant-table", q]) for q in range(9)]
        styles += [(name, ["-" + name]) for name in ("tune-psnr", "tune-ssim", "tune-ms-ssim", "notrellis")]
        for style, options in styles:
            name = f"moz-p{precision}-a{arithmetic}-{style}"
            run([paths[f"moz{precision}_cjpeg"], "-quality", 83, "-sample", "2x2,1x1,1x1", "-fastcrush",
                 *options, *(["-arithmetic"] if arithmetic else []), "-restart", "1B", "-outfile", image, source])
            check(name, image, {"kind": "mozjpeg custom settings", "style": style}, pin=style in ("separate", "refine", "quant8", "tune-ssim"))

    fixtures = Path(__file__).parent / "fixtures"
    skipped = []
    for family in ("sequential", "progressive", "arithmetic", "arithmetic-progressive", "arithmetic-model", "metadata"):
        for path in sorted((fixtures / family).glob("*.jpg")):
            precision, process = info(path.read_bytes())
            if precision == 12 and process in (0xc9, 0xca):
                skipped.append({"file": f"{family}/{path.name}", "reason": "Upstream WITH_12BIT disables arithmetic"})
                continue
            expected_file = path.with_suffix(".coef")
            check("replay-" + family + "-" + path.stem, path,
                  {"kind": "earlier retained fixture", "file": f"{family}/{path.name}"},
                  expected_file.read_bytes() if expected_file.exists() else None)
    retained = [row for row in records if row.get("retained")]
    report = {**identity, "reference": build["reference"], "source_commit": build["commit"],
              "scope": "Configured coefficient/raw/color differential gate; SIMD off. No conformance-corpus or performance claim. mozjpeg and Turbo share implementation ancestry.",
              "cases": len(records), "retained_images": len(retained), "skipped": skipped,
              "max_native_float_IDCT_error": max(r["native_float_IDCT"]["max"] for r in records if r["raw_IDCT_checked"]),
              "max_native_nearest_color_error": max(r.get("native_nearest_color", {"max": 0})["max"] for r in records),
              "results": records}
    (args.out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    if args.fixtures:
        (args.fixtures / "manifest.json").write_text(json.dumps({"scope": report["scope"], "images": retained}, indent=2) + "\n")
    print(json.dumps({k: v for k, v in report.items() if k not in ("results", "skipped", "tools")}), flush=True)


if __name__ == "__main__":
    main()
