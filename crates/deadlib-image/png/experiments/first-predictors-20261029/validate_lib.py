"""Record debug and release unit-oracle checks for the selected source."""
import hashlib
import json
from pathlib import Path
import subprocess

EVIDENCE = Path(__file__).resolve().parent
ROOT = EVIDENCE.parents[4]
PNG = ROOT / "crates/deadlib-image/png"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    manifest = EVIDENCE / "library-validation.json"
    assert not manifest.exists()
    source = {p.relative_to(ROOT).as_posix(): sha(p) for p in sorted((PNG / "src").rglob("*.rs"))}
    record = dict(source_sha256=source, commands=[])
    for profile in ["release", "debug"]:
        command = ["cargo", "test", "--offline", "-p", "deadlib-image-png", "--lib",
                   "--target-dir", str(ROOT / "target/png-first-predictors-20261029/build")]
        if profile == "release":
            command += ["--release"]
        command += ["--", "--nocapture"]
        path = EVIDENCE / ("library-" + profile + ".log")
        assert not path.exists()
        with path.open("wb") as stream:
            result = subprocess.run(command, cwd=ROOT, stdout=stream, stderr=subprocess.STDOUT)
        record["commands"].append(dict(command=command, exit_code=result.returncode,
                                       log=path.name, sha256=sha(path)))
        manifest.write_bytes((json.dumps(record, indent=2) + "\n").encode())
        assert result.returncode == 0, path.read_text()[-6000:]
        print(f"Library {profile} tests passed", flush=True)
    assert source == {p.relative_to(ROOT).as_posix(): sha(p) for p in sorted((PNG / "src").rglob("*.rs"))}


if __name__ == "__main__":
    main()
