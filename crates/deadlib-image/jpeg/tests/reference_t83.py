"""Official T.83 streams: exact C/native coefficients and lossless golden data.

DREF is reconstructed-sample/FDCT data, not entropy-decoded coefficients.
Numerical DREF/EREF checks run in Rust with independent f64 transforms.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import struct
import subprocess
from stream_support import marker_ranges


def sha(data):
    return hashlib.sha256(data).hexdigest()


def run(command):
    result = subprocess.run([str(v) for v in command], capture_output=True, timeout=120)
    assert result.returncode == 0 and not result.stderr, (command, result.returncode, result.stderr.decode(errors="replace"))


def lossless_reference(path, root, identifiers):
    data = path.read_bytes()
    assert data[:4] == b"JLP1"
    precision, width, height, count = struct.unpack_from("<4I", data, 4)
    assert (width, height, count) == (255, 257, 4)
    position, samples = 20, 0
    for index, label in enumerate("ABCD"):
        identifier, h, v, w, ht = struct.unpack_from("<5I", data, position)
        position += 20
        assert identifier == identifiers[index]
        assert (h, v) == ((1, 1), (1, 2), (3, 1), (1, 4))[index]
        assert (w, ht) == ((85, 65), (85, 129), (255, 65), (85, 257))[index]
        got = struct.unpack_from("<" + "H" * (w * ht), data, position)
        position += 2 * len(got)
        source = (root / f"{label}{8 if precision == 8 else 16}.SRC").read_bytes()
        expected = tuple(source) if precision == 8 else struct.unpack(">" + "H" * (len(source) // 2), source)
        assert got == expected, (path, label)
        samples += len(got)
    assert position == len(data)
    return samples


def main():
    parser = argparse.ArgumentParser()
    for name in ("native", "ijg", "turbo", "moz8", "moz12", "reference", "out", "fixtures"):
        parser.add_argument("--" + name, required=True, type=Path)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    tools = args.out / "tools"
    tools.mkdir(exist_ok=True)
    hashes = {}
    for name in ("native", "ijg", "turbo", "moz8", "moz12", "reference"):
        source = getattr(args, name)
        target = tools / (name + source.suffix)
        shutil.copy2(source, target)
        hashes[name] = sha(target.read_bytes())
        setattr(args, name, target.resolve())
    manifest = json.loads((args.fixtures / "manifest.json").read_text())
    for row in manifest["files"]:
        assert sha((args.fixtures / row["file"]).read_bytes()) == row["sha256"]
    records = []
    canonical = {}
    for image in sorted(args.fixtures.glob("*.JPG")):
        prefix = args.out / image.stem
        inputs = [image]
        if image.stem == "B2":
            inputs.insert(0, args.fixtures / "B1.JPG")
        run([args.native, "decode", prefix, "--max-scans", 2048, *inputs])
        frame = next(((marker, start) for marker, start, _ in marker_ranges(image.read_bytes())
                      if marker in (0xc0, 0xc1, 0xc2, 0xc3, 0xc9, 0xca, 0xcb)), None)
        row = {"file": image.name, "sha256": sha(image.read_bytes())}
        if frame is None:
            assert image.stem == "B1"
            row["kind"] = "table-only packet"
        elif frame[0] in (0xc3, 0xcb):
            row["kind"] = "predictive lossless"
            wire = image.read_bytes()
            start = frame[1]
            identifiers = [wire[start + 10 + 3 * i] for i in range(wire[start + 9])]
            row["exact_official_samples"] = lossless_reference(Path(str(prefix) + ".image-0.samples"), args.fixtures, identifiers)
            pgx = args.out / "pgx"
            run([args.reference, "-c", "-U", image, pgx])
            precision = wire[start + 4]
            for i, (label, (w, h)) in enumerate(zip("ABCD", ((85, 65), (85, 129), (255, 65), (85, 257)))):
                header = Path(str(pgx) + f"_{i}.h").read_text().split()
                assert header == ["PG", "ML", f"+{precision}", str(w), str(h)], header
                raw = Path(str(pgx) + f"_{i}.raw").read_bytes()
                assert raw == (args.fixtures / f"{label}{precision}.SRC").read_bytes(), (image.name, label)
            row["JPEG_XT_official_samples"] = "exact original stream, no header normalization"
        else:
            row["kind"] = "DCT"
            expected = Path(str(prefix) + ".image-0.coef").read_bytes()
            precision, width, height, count = struct.unpack_from("<4I", expected, 4)
            row.update({"precision": precision, "components": count, "coefficient_sha256": sha(expected)})
            ijg = args.out / "ijg.coef"
            run([args.ijg, *inputs, ijg])
            assert ijg.read_bytes() == expected, (image.name, "IJG")
            row["IJG"] = "exact genuine table/image pair" if count == 255 else "exact"
            if count <= 4:
                cprefix = args.out / "turbo"
                run([args.turbo, image, cprefix])
                assert cprefix.with_suffix(".coef").read_bytes() == expected, (image.name, "Turbo")
                row["Turbo"] = "exact"
                if precision == 12 and frame[0] in (0xc9, 0xca):
                    row["mozjpeg"] = "skipped: upstream 12-bit build disables arithmetic"
                else:
                    cprefix = args.out / "mozjpeg"
                    run([args.moz8 if precision == 8 else args.moz12, image, cprefix])
                    assert cprefix.with_suffix(".coef").read_bytes() == expected, (image.name, "mozjpeg")
                    row["mozjpeg"] = "exact"
                if precision in canonical:
                    assert canonical[precision] == expected, image.name
                else:
                    canonical[precision] = expected
            else:
                row["Turbo"] = "skipped: selector lookup beyond first four components"
                row["mozjpeg"] = "skipped: default component-count cap"
        records.append(row)
    assert len(records) == 21
    assert sum(row.get("exact_official_samples", 0) for row in records) == 219640
    expectations = []
    for precision, data in sorted(canonical.items()):
        name = f"observed-{precision}.coef"
        (args.fixtures / name).write_bytes(data)
        expectations.append({"file": name, "sha256": sha(data), "precision": precision,
                             "meaning": "Exact native and configured C entropy-decoder observation; not official DREF data"})
    report = {"scope": "Official T.83 original streams, exact coefficients across configured C APIs, exact lossless SRC references. Formal full conformance is not inferred from stream acceptance.",
              "archive_sha256": manifest["archive_sha256"], "tools": hashes,
              "driver_sha256": sha(Path(__file__).read_bytes()), "streams": 21, "images": 20,
              "exact_IJG_DCT_images": 16, "exact_Turbo_DCT_images": 15, "exact_mozjpeg_DCT_images": 11,
              "exact_official_lossless_samples": 219640, "expectations": expectations,
              "exact_JPEG_XT_lossless_images": 4,
              "results": records}
    (args.out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    (args.fixtures / "observations.json").write_text(json.dumps({"tools": hashes, "files": expectations}, indent=2) + "\n")
    print(json.dumps({k: v for k, v in report.items() if k not in ("tools", "results", "expectations")}), flush=True)


if __name__ == "__main__":
    main()
