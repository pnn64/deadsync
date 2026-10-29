"""File-stream output equality and independent entropy/frame-data checks.

Known-height DNL copies and JPEG XT EXP00 copies are explicit per case. This
checks streaming output; paired abbreviated output and input I/O are separate.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
from hierarchy_support import structure, check_frame_data, samples
from stream_support import marker_ranges, clear_stale_exp


def sha(data):
    return hashlib.sha256(data).hexdigest()


def run(command, env=None):
    result = subprocess.run([str(v) for v in command], capture_output=True, timeout=120, env=env)
    assert result.returncode == 0 and not result.stderr and b"warning" not in result.stdout.lower(), (command, result.returncode, result.stderr.decode(errors="replace"))


def tail(prefix, suffix):
    return Path(str(prefix) + suffix)


def known_heights(wire, header, frames):
    copy = bytearray(wire)
    index, remove = 0, []
    for marker, start, end in marker_ranges(wire):
        if marker == 0xde:
            copy[start + 5:start + 7] = header["height"].to_bytes(2, "big")
        elif marker in (0xc0, 0xc1, 0xc2, 0xc3, 0xc5, 0xc6, 0xc7, 0xc9, 0xca, 0xcb, 0xcd, 0xce, 0xcf):
            copy[start + 5:start + 7] = frames[index]["height"].to_bytes(2, "big")
            index += 1
        elif marker == 0xdc:
            remove.append((start, end))
    for start, end in reversed(remove):
        del copy[start:end]
    return bytes(copy)


def main():
    parser = argparse.ArgumentParser()
    for name in ("native", "ijg", "reference", "out"):
        parser.add_argument("--" + name, required=True, type=Path)
    parser.add_argument("--resume", action="store_true")
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    tools = args.out / "tools"
    tools.mkdir(exist_ok=True)
    hashes = {}
    for name in ("native", "ijg", "reference"):
        source = getattr(args, name)
        target = tools / (name + source.suffix)
        if not args.resume:
            shutil.copy2(source, target)
        hashes[name] = sha(target.read_bytes())
        assert hashes[name] == sha(source.read_bytes())
        setattr(args, name, target.resolve())
    root = Path(__file__).resolve().parent / "fixtures"
    sources = {}
    for group in ("sequential", "progressive", "arithmetic", "arithmetic-progressive", "lossless", "lossless-arithmetic", "hierarchical"):
        for source in sorted((root / group).glob("*.jpg")):
            _, frames, _, _, _ = structure(source.read_bytes(), False)
            frame = frames[-1]
            sources.setdefault((frame["marker"], frame["precision"]), [source])
    sessions = list(sources.values())
    # The official large abbreviated input and all four official lossless cases.
    sessions += [[root / "t83/B1.JPG", root / "t83/B2.JPG"]]
    sessions += [[root / "t83" / name] for name in ("O1.JPG", "O2.JPG", "P1.JPG", "P2.JPG")]
    # Retain independent many-component and distinct-quantizer coverage.
    manifest = json.loads((root / "components/manifest.json").read_text())
    for row in manifest["sessions"]:
        if row["case"].startswith("b5-255-"):
            sessions.append([root / "components" / record["file"] for record in row["streams"]])
    source_identity = [{"files": [str(p.relative_to(root)) for p in paths], "sha256": [sha(p.read_bytes()) for p in paths]} for paths in sessions]
    identity = {"tools": hashes, "driver_sha256": sha(Path(__file__).read_bytes()), "sources": source_identity}
    checkpoint = args.out / "checkpoint.jsonl"
    if args.resume:
        old = [json.loads(line) for line in checkpoint.read_text().splitlines()]
        assert old[0] == identity
        done = {row["case"]: row for row in old[1:]}
    else:
        checkpoint.write_text(json.dumps(identity) + "\n")
        done = {}
    work = args.out / "case"
    work.mkdir(exist_ok=True)
    records, processes = [], set()
    for si, paths in enumerate(sessions):
        for buffer, fixed, restart, dnl in ((1, 0, 1, 0), (31, 0, 1, 1), (8192, 1, 7, 0)):
            key = f"s{si}-b{buffer}-f{fixed}-r{restart}-d{dnl}"
            if key in done:
                records.append(done[key])
                processes.update(done[key]["processes"])
                continue
            (args.out / "current-case.json").write_text(json.dumps({"case": key, "sources": source_identity[si]}))
            prefix = work / "output"
            run([args.native, prefix, buffer, -1, -1, fixed, restart, dnl, *paths])
            image = tail(prefix, f".{len(paths)-1}.stream.jpg")
            memory = tail(prefix, f".{len(paths)-1}.memory.jpg")
            wire = image.read_bytes()
            assert wire == memory.read_bytes()
            header, frames, _, empty_dac, empty_exp = structure(wire, False)
            assert not empty_dac and not empty_exp
            markers = sorted({frame["marker"] for frame in frames})
            processes.update(markers)
            dumps = []
            if len(frames) == 1 and frames[0]["marker"] in (0xc0, 0xc1, 0xc2, 0xc9, 0xca):
                dumps.append(tail(prefix, f".{len(paths)-1}.expected.coef").read_bytes())
            elif len(frames) == 1:
                dumps.append(tail(prefix, f".{len(paths)-1}.expected.samples").read_bytes())
            else:
                for i, frame in enumerate(frames):
                    suffix = "samples" if frame["marker"] in (0xc3, 0xc7, 0xcb, 0xcf) else "coef"
                    dumps.append(tail(prefix, f".{len(paths)-1}.frame-{i}.{suffix}").read_bytes())
            c_wire = known_heights(wire, header, frames) if dnl else wire
            c_input = work / "reference.jpg"
            c_input.write_bytes(c_wire)
            ordinary_dct = len(frames) == 1 and frames[0]["marker"] in (0xc0, 0xc1, 0xc2, 0xc9, 0xca)
            if ordinary_dct:
                run([args.ijg, c_input, work / "ijg.coef"])
                assert (work / "ijg.coef").read_bytes() == dumps[0], key
            fractional = any(max(c[1] for c in f["components"]) % c[1] or max(c[2] for c in f["components"]) % c[2] for f in frames for c in f["components"])
            observation_keys = [(f["marker"], f["width"], f["height"]) for f in frames]
            duplicate_frames = len(set(observation_keys)) != len(observation_keys)
            blocks = 0
            exp_copy = False
            if not fractional and not duplicate_frames:
                adjusted = clear_stale_exp(c_wire, frames)
                exp_copy = adjusted != c_wire
                c_input.write_bytes(adjusted)
                observed = work / "reference.frames"
                observed.write_text("")
                pgx = work / "pgx"
                run([args.reference, "-c", "-U", c_input, pgx],
                    os.environ | {"DEADLIB_JPEG_FRAME_DUMP": str(observed.resolve())})
                if len(frames) == 1 and frames[0]["marker"] in (0xc3, 0xcb) and len(frames[0]["components"]) <= 4:
                    precision, _, _, planes = samples(dumps[0])
                    for i, (shape, values) in enumerate(planes):
                        w, h = shape[3:]
                        pgx_header = tail(pgx, f"_{i}.h").read_text().split()
                        assert pgx_header == ["PG", "ML", f"+{precision}", str(w), str(h)], pgx_header
                        raw = tail(pgx, f"_{i}.raw").read_bytes()
                        expected = bytes(values) if precision <= 8 else struct.pack(">" + "H" * len(values), *values)
                        assert raw == expected, (key, i, "PGX sample mismatch")
                    blocks, sample_count = 0, sum(len(values) for _, values in planes)
                else:
                    blocks, sample_count = check_frame_data(frames, dumps, observed.read_text())
                samples_checked = sample_count
            else:
                samples_checked = 0
            record = {"case": key, "sources": source_identity[si], "processes": markers,
                      "buffer_bytes": buffer, "fixed_Huffman": bool(fixed), "restart": restart,
                      "DNL": bool(dnl), "sha256": sha(wire), "bytes": len(wire), "memory_output_identical": True,
                      "IJG_exact": ordinary_dct, "JPEG_XT_exact": not fractional and not duplicate_frames,
                      "JPEG_XT_exclusion": "fractional parser limitation" if fractional else "observer cannot distinguish repeated same-kind same-size frames" if duplicate_frames else None,
                      "C_known_height_copy": bool(dnl), "C_EXP00_copy": exp_copy,
                      "C_DCT_blocks": blocks, "C_sample_values": samples_checked}
            records.append(record)
            with checkpoint.open("a") as f:
                f.write(json.dumps(record) + "\n")
            if len(records) % 25 == 0:
                print(json.dumps({"checked": len(records), "last": key}), flush=True)
    assert processes == {0xc0,0xc1,0xc2,0xc3,0xc5,0xc6,0xc7,0xc9,0xca,0xcb,0xcd,0xce,0xcf}
    report = {**identity, "scope": "Incremental file output, exact memory-output identity, IJG coefficients and JPEG XT raw frame data. Known-height DNL and EXP00 C-input copies are explicit; no speed claim.",
              "cases": len(records), "processes": sorted(processes),
              "IJG_exact_cases": sum(r["IJG_exact"] for r in records),
              "JPEG_XT_exact_cases": sum(r["JPEG_XT_exact"] for r in records),
              "C_DCT_blocks": sum(r["C_DCT_blocks"] for r in records),
              "C_sample_values": sum(r["C_sample_values"] for r in records),
              "C_known_height_copies": sum(r["C_known_height_copy"] for r in records),
              "C_EXP00_copies": sum(r["C_EXP00_copy"] for r in records), "results": records}
    (args.out / "report.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({k: v for k, v in report.items() if k not in ("results", "sources", "tools")}), flush=True)


if __name__ == "__main__":
    main()
