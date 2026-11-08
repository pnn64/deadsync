"""Retain the mozjpeg progressive scan-optimizer/restart producer defect.

Valid-stream gates allow no warnings. This separate probe expects warnings
from all C decoders and strict native rejection, then checks -fastcrush output.
"""
import argparse
import json
from pathlib import Path
import shutil
import subprocess
from reference_mozjpeg import samples, sha
from stream_support import marker_ranges

parser = argparse.ArgumentParser()
for name in ("build", "native", "turbo", "ijg", "out", "fixtures"):
    parser.add_argument("--" + name, required=True, type=Path)
args = parser.parse_args()
args.out.mkdir(parents=True, exist_ok=True)
args.fixtures.mkdir(parents=True, exist_ok=True)
build = json.loads(args.build.read_text())
tools = {k: Path(build["builds"][0]["tools"][k]["path"]) for k in ("cjpeg", "oracle")}
tools.update({k: getattr(args, k).resolve() for k in ("native", "turbo", "ijg")})
source = args.out / "source.ppm"
source.write_bytes(samples(8, 1))
records = []
for setting, options in (("optimized-restarts", []), ("fastcrush-restarts", ["-fastcrush"])):
    image = args.out / (setting + ".jpg")
    command = [tools["cjpeg"], "-quality", "20", "-progressive", "-sample", "2x1,1x1,1x1",
               "-restart", "1", *options, "-outfile", image, source]
    result = subprocess.run([str(v) for v in command], capture_output=True)
    assert result.returncode == 0 and not result.stderr
    record = {"setting": setting, "source_sha256": sha(source.read_bytes()),
              "image_sha256": sha(image.read_bytes()), "decoders": {}}
    coef = []
    for decoder in ("oracle", "turbo", "ijg", "native"):
        prefix = args.out / (setting + "-" + decoder)
        target = prefix.with_suffix(".coef") if decoder == "ijg" else prefix
        result = subprocess.run([str(tools[decoder]), str(image), str(target)], capture_output=True)
        stderr = result.stderr.decode().strip()
        if setting == "optimized-restarts":
            if decoder == "native":
                assert result.returncode != 0 and 'Invalid("entropy padding")' in stderr
            else:
                assert result.returncode == 0 and "extraneous bytes before marker" in stderr
        else:
            assert result.returncode == 0 and not stderr, (decoder, stderr)
            coef.append(prefix.with_suffix(".coef").read_bytes())
        record["decoders"][decoder] = {"returncode": result.returncode, "stderr": stderr}
    if coef:
        assert all(v == coef[0] for v in coef)
        record["coefficients"] = "exact across native and all three C decoders"
    shutil.copy2(image, args.fixtures / image.name)
    records.append(record)
# The combined-table writer also emits Lh=2 during DC refinements. T.81
# B.2.4.2 specifies one or more definitions; all C clients silently ignore it.
source.write_bytes(samples(8, 2))
script = args.out / "dc-refine.scans"
script.write_text("0,1,2: 0,0,0,2; 2: 1,63,0,2; 1: 1,63,0,2; 0: 1,63,0,2;\n" +
                  "".join(f"0,1,2: 0,0,{ah},{ah-1}; 2: 1,63,{ah},{ah-1}; 0: 1,63,{ah},{ah-1}; 1: 1,63,{ah},{ah-1};\n" for ah in (2, 1)))
original = args.out / "empty-dht.jpg"
result = subprocess.run([str(v) for v in [tools["cjpeg"], "-quality", 83, "-sample", "2x2,1x1,1x1",
                        "-scans", script, "-fastcrush", "-restart", "1B", "-outfile", original, source]], capture_output=True)
assert result.returncode == 0 and not result.stderr
wire = original.read_bytes()
removed = [(start, end) for marker, start, end in marker_ranges(wire) if marker == 0xc4 and end - start == 4]
assert len(removed) == 2
normalized = args.out / "without-empty-dht.jpg"
copy = bytearray(wire)
for start, end in reversed(removed):
    del copy[start:end]
normalized.write_bytes(copy)
expected = None
for image in (original, normalized):
    record = {"setting": image.stem, "source_sha256": sha(source.read_bytes()),
              "image_sha256": sha(image.read_bytes()), "decoders": {}}
    for decoder in ("oracle", "turbo", "ijg", "native"):
        prefix = args.out / (image.stem + "-" + decoder)
        target = prefix.with_suffix(".coef") if decoder == "ijg" else prefix
        result = subprocess.run([str(tools[decoder]), str(image), str(target)], capture_output=True)
        stderr = result.stderr.decode().strip()
        if image == original and decoder == "native":
            assert result.returncode != 0 and 'Invalid("empty DHT")' in stderr
        else:
            assert result.returncode == 0 and not stderr, (decoder, stderr)
            coefficients = prefix.with_suffix(".coef").read_bytes()
            if expected is None:
                expected = coefficients
            assert coefficients == expected
        record["decoders"][decoder] = {"returncode": result.returncode, "stderr": stderr}
    if image == normalized:
        record["removed_empty_DHT_original_offsets"] = removed
        record["coefficients"] = "exact against all original C decodes; entropy unchanged"
    shutil.copy2(image, args.fixtures / image.name)
    records.append(record)
report = {"scope": "Explicit reference-producer limitations; C warnings expected only for optimized-restarts. Empty DHTs are silently ignored by C; native rejects them. Explicit removal copy leaves entropy unchanged.",
          "reference_commit": build["commit"], "archive_sha256": build["archive_sha256"],
          "tools": {k: sha(v.read_bytes()) for k, v in tools.items()},
          "driver_sha256": sha(Path(__file__).read_bytes()),
          "results": records}
(args.out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
(args.fixtures / "manifest.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps({"cases": len(records), "expected_rejection": True, "fixed_setting_exact": True}))
