"""Verify the codec and shared primitives build without the application workspace."""
from pathlib import Path
import shutil
import subprocess
import tempfile

codec = Path(__file__).resolve().parents[1]
shared = codec.parent
temporary = tempfile.TemporaryDirectory(prefix="deadlib-jpeg-independent-")
root = Path(temporary.name).resolve()
# Verify the recursive cleanup target created by TemporaryDirectory before use.
assert root.parent == Path(tempfile.gettempdir()).resolve()
assert root.name.startswith("deadlib-jpeg-independent-")
try:
    target = root / "deadlib-image"
    target.mkdir()
    shutil.copy2(shared / "Cargo.toml", target / "Cargo.toml")
    shutil.copytree(shared / "src", target / "src")
    standalone = target / "jpeg"
    standalone.mkdir()
    shutil.copy2(codec / "Cargo.toml", standalone / "Cargo.toml")
    for name in ("src", "tests", "examples"):
        shutil.copytree(codec / name, standalone / name, ignore=shutil.ignore_patterns("__pycache__"))
    subprocess.run(["cargo", "test", "--manifest-path", str(standalone / "Cargo.toml")],
                   cwd=root, check=True)
    print("Independent package build and tests passed (only deadlib-image dependency).")
finally:
    temporary.cleanup()
