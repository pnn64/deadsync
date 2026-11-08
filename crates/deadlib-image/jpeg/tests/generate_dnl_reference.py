"""Author DNL inputs with the pinned independent JPEG XT encoder (-n).

The only syntax repairs remove its documented empty DAC/EXP00 segments.
DNL, zero SOF/DHP heights and every entropy byte remain untouched.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import struct
import subprocess
from hierarchy_support import structure

parser = argparse.ArgumentParser()
parser.add_argument("--reference", type=Path, required=True)
parser.add_argument("--out", type=Path, required=True)
args = parser.parse_args()
args.out.mkdir(parents=True, exist_ok=True)
reference = args.out / "reference.exe"
shutil.copy2(args.reference, reference)
reference = reference.resolve()
sources = args.out / "sources"
sources.mkdir(exist_ok=True)
records = []
for lossless in (False, True):
    for p in (range(2, 17) if lossless else (8, 12)):
        for arithmetic in (False, True):
            for progressive in ((False,) if lossless else (False, True)):
                for color in (("gray", "rgb444") if lossless else ("gray", "rgb444", "rgb420")):
                    for level in (None, 2):
                        for restart in (0, 5):
                            name = f"{'lp' if lossless else 'prog' if progressive else 'seq'}-p{p}-a{int(arithmetic)}-{color}-y{level}-r{restart}"
                            width, height = 17, 24
                            count = 1 if color == "gray" else 3
                            maximum = (1 << p) - 1
                            values = [(maximum * (c + 1) // (count + 1)) if restart == 0 else (x * 7319 + y * y * 117 + c * 771 + x * y * 131) & maximum
                                      for y in range(height) for x in range(width) for c in range(count)]
                            raw = bytes(values) if p <= 8 else struct.pack(">" + "H" * len(values), *values)
                            pnm = f"P{5 if count == 1 else 6}\n{width} {height}\n{maximum}\n".encode() + raw
                            source = args.out / "source.pnm"
                            source.write_bytes(pnm)
                            original = args.out / "original.jpg"
                            # Annex H restarts must end on complete MCU rows.
                            # Hierarchical widths are 9 and 17; 153 is a row
                            # multiple for both. DCT permits arbitrary MCUs.
                            interval = (153 if level is not None else width * restart) if lossless and restart else restart
                            command = [reference, "-c", "-n", "-s", "1x1,2x2,2x2" if color == "rgb420" else "1x1,1x1,1x1",
                                       *(["-p"] if lossless else ["-q", "85"]), "-a" if arithmetic else "-h",
                                       *(["-v"] if progressive else []), *(["-y", str(level)] if level is not None else []),
                                       "-z", str(interval), source, original]
                            result = subprocess.run([str(v) for v in command], capture_output=True)
                            error = result.stderr.decode(errors="replace").strip()
                            record = {"case": name, "source_sha256": hashlib.sha256(pnm).hexdigest(), "restart_mcus": interval, "reference_encoder_error": error or None}
                            if error:
                                assert lossless and not arithmetic and "Huffman table is unsuitable" in error, (name, error)
                            else:
                                assert result.returncode == 0 and b"warning" not in result.stdout.lower()
                                encoded = original.read_bytes()
                                header, frames, normalized, dac, exp = structure(encoded, False)
                                assert all(f["dnl_segments"] == 1 and f["declared_height"] == 0 for f in frames)
                                (sources / (name + ".jpg")).write_bytes(normalized)
                                record |= {"sha256": hashlib.sha256(normalized).hexdigest(), "original_sha256": hashlib.sha256(encoded).hexdigest(),
                                           "bytes": len(normalized), "empty_DAC_removed": dac, "EXP00_removed": exp,
                                           "DNL_segments": len(frames), "unknown_DHP": header["marker"] == 0xde and header["declared_height"] == 0,
                                           "markers": [f["marker"] for f in frames]}
                            records.append(record)
                            if len(records) % 50 == 0:
                                print(f"Attempted {len(records)} independent DNL encodes", flush=True)
report = {"reference": "thorfdbg/libjpeg JPEG XT 702114c8130ae818ccd2354cc03e879dac69e579",
          "reference_executable_sha256": hashlib.sha256(reference.read_bytes()).hexdigest(), "attempts": len(records),
          "encoded_sources": sum(r["reference_encoder_error"] is None for r in records),
          "encoder_failures": sum(r["reference_encoder_error"] is not None for r in records),
          "scope": "Independent C-authored DNL headers and entropy. Syntax repairs remove only empty DAC and EXP00, not DNL or zero heights. No source-fidelity claim for the reference hierarchical encoder.",
          "results": records}
(args.out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
print(json.dumps({k: v for k, v in report.items() if k != "results"}))
