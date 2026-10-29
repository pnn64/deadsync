"""Pin the official ITU T.83 test signals; never redistribute the standard text.

Only the unique compressed streams and numeric SRC/DCT data are retained.
The archive is downloaded to the caller's work directory, hash-checked, and
read without extracting arbitrary paths. All retained bytes stay unchanged.
"""
import argparse
import hashlib
import json
from pathlib import Path
import urllib.request
import zipfile

URL = "https://www.itu.int/wftp3/public/t/testsignal/SpeImage/T83/v1994_11/T83_data_files.zip"
SHA256 = "9f5ed0a74a3087c6c93f4cbb0bd5128c2a87b128e4f59f5b6649e32941da7069"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--work", required=True, type=Path)
    parser.add_argument("--fixtures", required=True, type=Path)
    args = parser.parse_args()
    args.work.mkdir(parents=True, exist_ok=True)
    args.fixtures.mkdir(parents=True, exist_ok=True)
    archive = args.work / "T83_data_files.zip"
    if not archive.exists():
        archive.write_bytes(urllib.request.urlopen(URL, timeout=45).read())
    assert hashlib.sha256(archive.read_bytes()).hexdigest() == SHA256
    records = {}
    with zipfile.ZipFile(archive) as z:
        for entry in z.infolist():
            name = Path(entry.filename).name
            if Path(name).suffix.upper() not in (".JPG", ".SRC", ".DCT"):
                continue
            data = z.read(entry)
            digest = hashlib.sha256(data).hexdigest()
            if name in records:
                assert records[name]["sha256"] == digest, entry.filename
                records[name]["archive_members"].append(entry.filename)
                continue
            target = args.fixtures / name
            assert target.parent == args.fixtures
            target.write_bytes(data)
            records[name] = {"file": name, "bytes": len(data), "sha256": digest,
                             "archive_members": [entry.filename]}
    assert len(records) == 45
    assert sum(name.endswith(".JPG") for name in records) == 21
    manifest = {"source": URL, "archive_sha256": SHA256, "archive_bytes": archive.stat().st_size,
                "catalogue": "https://www.itu.int/ITU-T/recommendations/rec.aspx?id=2635",
                "public_file_catalogue": "https://www.itu.int/mws/api/testsignals/signalfiles?ts_id=6",
                "copyright": "ITU & ISO/IEC 1994; see the original archive Readme.txt",
                "scope": "Unmodified official compressed streams and numeric references. Twenty images and one table packet; absent test streams remain absent. No claim of exhaustive conformance.",
                "files": sorted(records.values(), key=lambda row: row["file"])}
    (args.fixtures / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    print(json.dumps({"files": len(records), "unique_streams": 21, "numeric_references": 24,
                      "archive_sha256": SHA256}))


if __name__ == "__main__":
    main()
