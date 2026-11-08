"""Bridge the current oracle CLI to earlier exact C-checked paired bytes.

This reruns native file emission and its vector comparison, without C decoding.
It also audits that preparing each original input once before repeating its
model preserves all earlier self-contained-source session outputs.
"""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--native", required=True, type=Path)
    parser.add_argument("--out", required=True, type=Path)
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    native = args.out / "native.exe"
    shutil.copy2(args.native, native)
    assert native.read_bytes() == args.native.read_bytes()
    native = native.resolve()
    tests = Path(__file__).resolve().parent
    records, baselines = [], {}
    for part in ("native", "rare"):
        baseline = tests / f"reports/abbreviated-{part}.json"
        baselines[baseline.name] = sha(baseline.read_bytes())
        for row in json.loads(baseline.read_text())["results"]:
            sources = [tests / "fixtures" / path for path in row["source_files"]]
            assert [sha(p.read_bytes()) for p in sources] == row["sources"]
            for buffer in (0,31,8192):
                prefix = args.out / "image"
                command = [native,"encode-stream",buffer,0,prefix,row["fixed"],row["restart"],2,*sources]
                result = subprocess.run([str(x) for x in command],capture_output=True,timeout=180)
                assert not result.returncode and not result.stderr, (row["case"],buffer,result.stderr.decode(errors="replace"))
                assert result.stdout.decode().strip() == f"images {row['images']}"
                output = b"".join(Path(str(prefix)+f".image-{i}{tail}").read_bytes() for i in range(row["images"]) for tail in (".tables.jpg",".jpg"))
                assert sha(output)==row["stream_sha256"] and len(output)==row["bytes"], (part,row["case"],buffer)
                records.append({"part":part,"case":row["case"],"buffer_bytes":buffer,"images":row["images"],"bytes":len(output),"stream_sha256":sha(output),"file_vector_identical":True,"prior_C_bytes_identical":True,"C_decoding_rerun":False})
            if len(records)%30==0:
                print(json.dumps({"sessions":len(records),"last":row["case"]}),flush=True)
    assert len(records)==180 and sum(r["images"] for r in records)==10368
    report={"scope":"Current source-context-aware CLI replay; exact paired bytes reuse earlier C evidence, without fresh C decoding.","native_sha256":sha(native.read_bytes()),"driver_sha256":sha(Path(__file__).read_bytes()),"baseline_reports":baselines,"sessions":len(records),"image_outputs":10368,"C_decoding_rerun":False,"results":records}
    (args.out/"report.json").write_text(json.dumps(report,indent=2)+"\n")
    print(json.dumps({k:v for k,v in report.items() if k!="results"}),flush=True)


if __name__=="__main__":
    main()
