"""Index only Git-tracked song archives for hermetic regression inputs.

Run with python -X utf8 scripts/catalog_committed_songs.py (requires zstandard).
Only the first tar member, manifest.json, is decoded; song assets stay packed.
"""
import json
from pathlib import Path
import subprocess
import tarfile

import zstandard


def main():
    root = Path(__file__).resolve().parent.parent
    store = root / "tests/fixtures/full_song_lua"
    tracked = subprocess.check_output(
        ["git", "ls-files", "--", "tests/fixtures/full_song_lua/*.tar.zst"],
        cwd=root, text=True, encoding="utf-8").splitlines()
    names = sorted(Path(name).name for name in tracked)
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
    tracked_reference_files = set(subprocess.check_output(
        ["git", "ls-files", "--", "tests/fixtures/full_song_lua"],
        cwd=root, text=True, encoding="utf-8").splitlines())
    reference_names = sorted(set(references.values()))
    for name in reference_names:
        assert (store / name).relative_to(root).as_posix() in tracked_reference_files, (
            f"reference target is not tracked: {name}; add a repository fixture or remove the stale alias")
    (store / "committed-reference-files.txt").write_bytes(
        ("\n".join(reference_names) + "\n").encode("utf-8"))
    print(f"Cataloged {len(songs)} committed song archives")


if __name__ == "__main__":
    main()
