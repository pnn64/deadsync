"""Run independent container, codec, streaming and seek checks on a pinned song pack."""
import argparse
import datetime
import json
import os
from pathlib import Path
import subprocess

from fetch_songpacks import digest, matches


def main():
    decode = Path(__file__).resolve().parent.parent
    repo = decode.parents[2]
    reference = repo / "target/deadlib-audio-reference"
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pack", default="club-fantastic-season2")
    parser.add_argument("--output", type=Path, required=True, help="new validation directory")
    parser.add_argument("--target-dir", type=Path, default=repo / "target/vorbis-completion")
    args = parser.parse_args()
    manifest = json.loads((decode / "vorbis/validation/songpacks.json").read_text(encoding="utf8"))
    pack = next(p for p in manifest["packs"] if p["name"] == args.pack)
    corpus = reference / "corpus" / pack["name"]
    for entry in pack["files"]:
        if not matches(corpus / entry["member"], entry):
            raise ValueError(f"missing/changed input: {entry['member']}; run fetch_songpacks.py")
    expected = {e["member"] for e in pack["files"] if e["role"] == "audio"}
    actual = {p.relative_to(corpus).as_posix() for p in corpus.rglob("*.ogg")}
    if expected != actual:
        raise ValueError("song pack corpus must contain exactly the pinned audio files")
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    env = os.environ.copy()
    env["DEADLIB_VORBIS_CORPUS"] = env["DEADLIB_OGG_CORPUS"] = str(corpus)
    oracles = {}
    for package in ["ogg", "vorbis"]:
        key = "DEADLIB_LIB" + package.upper() + "_ORACLE"
        if key in env:
            oracle = Path(env[key])
        else:
            name = "deadlib_" + package + "_oracle" + (".exe" if os.name == "nt" else "")
            candidates = [reference / (package + "-oracle-build") / p / name for p in ["Release", ""]]
            oracle = next((p for p in candidates if p.is_file()), None)
            if oracle is None:
                raise ValueError(f"build the pinned oracle or set {key}")
        env[key] = str(oracle)
        oracles[key] = digest(oracle)
    for key, name in [("DEADLIB_VORBIS_REPORT", "raw-pcm.tsv"), ("DEADLIB_VORBIS_MAPPING_REPORT", "ogg-pcm.tsv"), ("DEADLIB_VORBIS_SEEK_REPORT", "seeks.tsv")]:
        env[key] = str(output / name)
    jobs = [
        ("container", "deadlib-audio-decode-ogg", "libogg", "libogg_local_corpus"),
        ("headers", "deadlib-audio-decode-vorbis", "headers", "local_corpus_headers_match_symphonia"),
        ("codebooks", "deadlib-audio-decode-vorbis", "libvorbis", "libvorbis_local_corpus_headers_codebooks_and_huffman"),
        ("raw-pcm", "deadlib-audio-decode-vorbis", "pcm", "local_corpus_pcm_matches_libvorbis_and_symphonia"),
        ("ogg-pcm", "deadlib-audio-decode-vorbis", "ogg_mapping", "libvorbisfile_local_corpus_final_counts_and_pcm"),
        ("streaming-reset", "deadlib-audio-decode-vorbis", "ogg_mapping", "local_corpus_fragmented_streaming_and_reset"),
        ("retained-seeks", "deadlib-audio-decode-vorbis", "seeking", "libvorbisfile_local_corpus_repeated_sample_accurate_seeks"),
        ("reference-seeks", "deadlib-audio-decode-vorbis", "reference_seeking", "reference_seeks_match_asset_corpus"),
    ]
    sources = {p.relative_to(decode).as_posix(): digest(p) for part in ["ogg/src", "vorbis/src", "ogg/tests", "vorbis/tests"] for p in sorted((decode / part).rglob("*.rs"))}
    receipt = {"started_utc": datetime.datetime.now(datetime.timezone.utc).isoformat(), "git_head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip(), "rustc": subprocess.check_output(["rustc", "-Vv"], text=True), "pack": pack, "oracles": oracles, "source_sha256": sources, "commands": []}
    for label, package, test, name in jobs:
        command = ["cargo", "test", "-p", package, "--release", "--all-features", "--target-dir", str(args.target_dir), "--test", test, name, "--", "--ignored", "--exact", "--nocapture"]
        print("running", label, flush=True)
        log = output / (label + ".log")
        with log.open("wb") as destination:
            result = subprocess.run(command, cwd=repo, env=env, stdout=destination, stderr=subprocess.STDOUT)
        record = {"label": label, "command": command, "exit_code": result.returncode, "log_sha256": digest(log)}
        receipt["commands"].append(record)
        (output / "checks.json").write_bytes((json.dumps(receipt, indent=2) + "\n").encode())
        if result.returncode or "test result: ok. 1 passed; 0 failed" not in log.read_text(encoding="utf8", errors="replace"):
            raise RuntimeError(f"failed {label}: {log}")
        print("passed", label, flush=True)
    for name, value in sources.items():
        if digest(decode / name) != value:
            raise RuntimeError(f"source changed during validation: {name}")
    receipt["completed_utc"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
    (output / "checks.json").write_bytes((json.dumps(receipt, indent=2) + "\n").encode())
    print("Song pack validation retained at", output, flush=True)


if __name__ == "__main__":
    main()
