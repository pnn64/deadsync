"""Verify and archive the exact source transitions recorded by the benchmarks."""
import difflib
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[4]
PNG = ROOT / "crates/deadlib-image/png"
EVIDENCE = PNG / "experiments/reconstruction-20261028"


def main():
    labels = ["baseline", "scale", "packed", "first-row"]
    records = {label: json.loads((EVIDENCE / f"{label}.json").read_text()) for label in labels}
    baseline = records["baseline"]
    versions = {}
    for name in baseline["source_sha256"]:
        path = PNG / name
        original = subprocess.check_output(["git", "show", baseline["commit"] + ":" + path.relative_to(ROOT).as_posix()], cwd=ROOT)
        versions[name] = original.decode().replace("\r\n", "\n")
    snapshots = {"baseline": versions.copy()}
    versions["src/composite.rs"] = (PNG / "src/composite.rs").read_text()
    snapshots["scale"] = versions.copy()
    scanline = versions["src/scanline.rs"]
    marker = "pub(crate) fn decode(\n"
    unpack = """fn unpack<const DEPTH: u8>(row: &[u8], out: &mut [u8]) {
    for (samples, &byte) in out.chunks_mut(8 / DEPTH as usize).zip(row) {
        for (n, sample) in samples.iter_mut().enumerate() {
            *sample = (byte >> (8 - DEPTH - n as u8 * DEPTH)) & ((1 << DEPTH) - 1);
        }
    }
}

"""
    assert scanline.count(marker) == 1
    scanline = scanline.replace(marker, unpack + marker)
    scanline = scanline.replace("if dx == 1 && depth >= 8 {", "if dx == 1 {")
    scanline = scanline.replace(
        "                    Samples::U8(v) => v[dst..dst + row.len()].copy_from_slice(row),",
        """                    Samples::U8(v) => {
                        let out = &mut v[dst..dst + w * channels];
                        match depth {
                            1 => unpack::<1>(row, out),
                            2 => unpack::<2>(row, out),
                            4 => unpack::<4>(row, out),
                            _ => out.copy_from_slice(row),
                        }
                    }""")
    versions["src/scanline.rs"] = scanline
    snapshots["packed"] = versions.copy()
    versions["src/scanline.rs"] = (PNG / "src/scanline.rs").read_text()
    snapshots["first-row"] = versions.copy()
    evidence = {}
    for label in labels:
        for name, text in snapshots[label].items():
            alternatives = [text.encode(), text.replace("\n", "\r\n").encode()]
            expected = records[label]["source_sha256"][name]
            assert any(hashlib.sha256(data).hexdigest() == expected for data in alternatives), (label, name)
        evidence[label] = dict(all_source_hashes_verified=True)
    for before, after in zip(labels, labels[1:]):
        patch = ""
        for name in versions:
            patch += "".join(difflib.unified_diff(snapshots[before][name].splitlines(keepends=True),
                                                snapshots[after][name].splitlines(keepends=True),
                                                fromfile="a/" + name, tofile="b/" + name))
        path = EVIDENCE / f"{before}-to-{after}.patch"
        path.write_bytes(patch.encode())
        evidence[after]["patch_from_previous"] = path.name
        evidence[after]["patch_sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()
    (EVIDENCE / "checkpoints.json").write_text(json.dumps(evidence, indent=2) + "\n")
    print("Every benchmark checkpoint matches its archived source transition.")


if __name__ == "__main__":
    main()
