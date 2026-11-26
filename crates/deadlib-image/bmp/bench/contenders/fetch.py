"""Fetch pinned BMP benchmark inputs and foreign codec build dependencies."""

import argparse
import hashlib
import subprocess
import zipfile
from pathlib import Path


def archive(root, name, url, expected, folder):
    path = root / name
    if not path.exists():
        subprocess.run(["curl.exe", "-fLsS", "--retry", "2", "-o", path, url], check=True)
    with path.open("rb") as stream:
        if hashlib.file_digest(stream, "sha256").hexdigest() != expected:
            raise ValueError(f"SHA-256 mismatch for {path}")
    if path.suffix == ".zip":
        with zipfile.ZipFile(path) as source:
            source.extractall(folder)
    else:
        subprocess.run(["tar.exe", "-xf", path, "-C", folder], check=True)


def sources(root, name, url, revision, paths):
    objects = root / f"{name}-objects"
    dest = root / name
    if not (objects / "HEAD").is_file():
        subprocess.run(["git", "init", "--bare", objects], check=True)
    exists = subprocess.run(["git", "-C", objects, "cat-file", "-e", revision],
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if exists.returncode:
        subprocess.run(["git", "-C", objects, "fetch", "--depth=1", url, revision], check=True)
    for name in paths:
        path = dest / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(subprocess.check_output(["git", "-C", objects, "show", f"{revision}:{name}"]))


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("work", type=Path)
    args = ap.parse_args()
    root = args.work.resolve() / "upstream"
    root.mkdir(parents=True, exist_ok=True)
    sources(root, "wuffs", "https://github.com/google/wuffs.git",
            "a87f0bb85b9406d29f48a1c03b6f42c4585145f6", ["release/c/wuffs-v0.4.c"])
    sources(root, "stb", "https://github.com/nothings/stb.git",
            "6b5799bae8a1fe3792691ef4b72ae26434c3e799", ["stb_image.h", "stb_image_write.h"])
    sources(root, "nsbmp-source", "https://github.com/netsurf-browser/libnsbmp.git",
            "3406e7e0bfb5a8edc73af1cd4ae39553907514fb", ["include/libnsbmp.h", "src/libnsbmp.c", "src/utils/log.h"])
    archive(root, "sdl.zip", "https://github.com/libsdl-org/SDL/releases/download/release-3.4.20/SDL3-devel-3.4.20-VC.zip",
            "1ec3009655ee37b1f73e52b3b120acc5afd8abaf9532f3303b1bf1dfe91b5969", root / "sdl")
    archive(root, "suite.zip", "https://entropymine.com/jason/bmpsuite/releases/bmpsuite-2.8.zip",
            "f1b1f18c4f310b76bf317378e64f25b84f875a3d0976fab33ff10befdb35d13f", root / "suite")
    archive(root, "ffmpeg.7z", "https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-9.1-full_build-shared.7z",
            "6bc6f09cd4379ed914489728044c1e0115861a9d47f7a1c07d71b8bec02708d7", root)


if __name__ == "__main__":
    main()
