"""Reject fixture Git history containing blobs of 50,000,000 bytes or more."""
from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parents[1]
objects = subprocess.check_output(["git", "rev-list", "--objects", "--all"], cwd=root)
sizes = subprocess.check_output(
    ["git", "cat-file", "--batch-check=%(objecttype) %(objectsize) %(rest)"],
    cwd=root, input=objects)
largest = 0
failures = []
for line in sizes.splitlines():
    kind, size, *name = line.split(b" ", 2)
    if kind == b"blob":
        size = int(size)
        largest = max(largest, size)
        if size >= 50_000_000:
            failures.append((size, name[0].decode("utf-8", errors="replace") if name else "unknown"))
for size, name in failures:
    print(f"Oversized Git blob: {size:,} bytes {name}", file=sys.stderr)
if failures:
    print("Split large archives with DeadSync's scripts/prepare_test_fixtures.py --split.", file=sys.stderr)
    raise SystemExit(1)
print(f"Fixture history size check passed; largest blob: {largest:,} bytes")
