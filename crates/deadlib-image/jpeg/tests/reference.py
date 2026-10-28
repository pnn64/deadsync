"""Generate reproducible sequential fixtures and compare native/C outputs.

Test only; Python standard library and separately built libjpeg tools suffice.
The CMYK fixture is optional and uses Pillow when --pillow is requested.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess


def run(args):
    return subprocess.run([str(a) for a in args], check=True, capture_output=True)


def samples(precision, width=31, height=25, channels=3):
    maximum = (1 << precision) - 1
    values = [((x * 193 + y * 317 + c * 911 + (x * y * 53) % 997) * maximum // 4095) & maximum
              for y in range(height) for x in range(width) for c in range(channels)]
    payload = bytes(values) if precision == 8 else struct.pack(">" + "H" * len(values), *values)
    return f"P{6 if channels == 3 else 5}\n{width} {height}\n{maximum}\n".encode() + payload


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--cjpeg", required=True, type=Path)
    parser.add_argument("--oracle", required=True, type=Path)
    parser.add_argument("--native", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    parser.add_argument("--pillow", action="store_true")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    cases = []
    for precision in (8, 12):
        src = args.out / f"source-{precision}.ppm"
        src.write_bytes(samples(precision))
        for name, options in [
            ("444", ["-sample", "1x1,1x1,1x1"]),
            ("422", ["-sample", "2x1,1x1,1x1"]),
            ("420", ["-sample", "2x2,1x1,1x1"]),
            ("411", ["-sample", "4x1,1x1,1x1"]),
            ("440", ["-sample", "1x2,1x1,1x1"]),
            ("410", ["-sample", "4x2,1x1,1x1"]),
            ("rgb", ["-rgb"]),
            ("gray", ["-grayscale"]),
            ("restart", ["-restart", "1B", "-sample", "1x1,1x1,1x1"]),
            ("optimized", ["-optimize"]),
            ("wide-quant", ["-quality", "1"]),
        ]:
            path = args.out / f"{precision}-{name}.jpg"
            command = [args.cjpeg, "-precision", precision, "-quality", 83, *options,
                       "-outfile", path, src]
            run(command)
            cases.append((path, command))
        script = args.out / "sequential.scans"
        script.write_text("0; 1; 2;\n")
        path = args.out / f"{precision}-multiscan.jpg"
        command = [args.cjpeg, "-precision", precision, "-scans", script,
                   "-restart", "2B", "-outfile", path, src]
        run(command)
        cases.append((path, command))
    if args.pillow:
        import PIL
        from PIL import Image
        path = args.out / "8-cmyk.jpg"
        image = Image.new("CMYK", (31, 25))
        image.putdata([tuple((x * 19 + y * 13 + c * 71) % 256 for c in range(4))
                       for y in range(25) for x in range(31)])
        image.save(path, quality=83)
        cases.append((path, ["Pillow", PIL.__version__, "CMYK", "quality=83"]))
    manifest = {"reference": run([args.cjpeg, "-version"]).stderr.decode().strip(), "cases": []}
    for path, command in cases:
        reference = path.with_suffix("")
        native = path.with_name(path.stem + "-native")
        run([args.oracle, path, reference])
        run([args.native, path, native])
        assert reference.with_suffix(".coef").read_bytes() == native.with_suffix(".coef").read_bytes(), path
        ref = reference.with_suffix(".planes").read_bytes()
        got = native.with_suffix(".planes").read_bytes()
        assert len(ref) == len(got), path
        a = struct.unpack("<" + "H" * (len(ref) // 2), ref)
        b = struct.unpack("<" + "H" * (len(got) // 2), got)
        errors = [abs(x - y) for x, y in zip(a, b)]
        maximum = max(errors)
        assert maximum <= 1, (path, maximum)
        record = {"file": path.name, "command": [str(a) for a in command],
                  "coefficient_match": True, "max_idct_error": maximum,
                  "mean_idct_error": sum(errors) / len(errors),
                  "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
        manifest["cases"].append(record)
        for suffix in (".coef", ".planes", ".pixels"):
            native.with_suffix(suffix).unlink(missing_ok=True)
        # Native color conversion output is experimental; component IDCT is
        # validated here. RGB conversion has its own gate in later milestones.
    (args.out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps({"cases": len(cases), "coefficients": "exact", "max_idct_error":
                      max(c["max_idct_error"] for c in manifest["cases"])}))


if __name__ == "__main__":
    main()
