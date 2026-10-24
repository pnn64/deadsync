"""Index only Git-tracked song archives for hermetic regression inputs.

Run with python -X utf8 scripts/catalog_committed_songs.py (requires zstandard).
Only the first tar member, manifest.json, is decoded; song assets stay packed.
"""
import json
from pathlib import Path
import subprocess
import tarfile

import zstandard

from prepare_test_fixtures import load_archives, prepare


def main():
    root = Path(__file__).resolve().parent.parent
    store = root / "tests/fixtures/full_song_lua"
    prepare(store)
    tracked = subprocess.check_output(
        ["git", "ls-files", "--", "*.tar.zst"],
        cwd=store, text=True, encoding="utf-8").splitlines()
    tracked_files = set(subprocess.check_output(
        ["git", "ls-files", "--"], cwd=store, text=True, encoding="utf-8").splitlines())
    split_archives = load_archives(store)
    if split_archives:
        assert "split-archives.json" in tracked_files, "add the split archive manifest to Git"
    for entry in split_archives:
        assert all(part["file"] in tracked_files for part in entry["parts"]), (
            f"split archive has untracked parts: {entry['archive']}")
    names = sorted({Path(name).name for name in tracked} | {entry["archive"] for entry in split_archives})
    songs = []
    for name in names:
        with (store / name).open("rb") as packed:
            with zstandard.ZstdDecompressor().stream_reader(packed) as decoded:
                with tarfile.open(fileobj=decoded, mode="r|") as archive:
                    member = next(iter(archive))
                    assert member.name == "manifest.json", name
                    manifest = json.load(archive.extractfile(member))
        songs.append({
            "title": manifest["chart"]["title"],
            "source_simfile": manifest["chart"]["source_path"],
            "harness_version": manifest["harness_version"],
            "archive": name,
        })
    (store / "committed-archives.txt").write_text("\n".join(names) + "\n", encoding="utf-8")
    (store / "committed-songs.json").write_text(
        json.dumps(songs, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    references = json.loads((store / "references.json").read_bytes())
    tracked_reference_files = tracked_files | {entry["archive"] for entry in split_archives}
    reference_names = sorted(set(references.values()))
    for name in reference_names:
        assert name in tracked_reference_files, (
            f"reference target is not tracked: {name}; add a repository fixture or remove the stale alias")
    (store / "committed-reference-files.txt").write_bytes(
        ("\n".join(reference_names) + "\n").encode("utf-8"))
    print(f"Cataloged {len(songs)} committed song archives")


if __name__ == "__main__":
    main()
