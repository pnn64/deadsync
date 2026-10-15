#!/usr/bin/env python3
"""Losslessly seal inactive VM traces/reports; never remove a sole capture.

Run on the Linux parity VM after jobs, or use --watch for periodic maintenance.
Each removed raw file has a verified .zst beside it and a durable JSONL receipt.
Restore with: zstd -d --keep FILE.zst
"""

import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time


def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def decoded_sha(path):
    child = subprocess.Popen(['zstd', '-q', '-d', '-c', str(path)], stdout=subprocess.PIPE)
    digest = hashlib.file_digest(child.stdout, 'sha256').hexdigest()
    child.stdout.close()
    if child.wait():
        raise RuntimeError(f'Cannot decode {path}')
    return digest


def process_references(target, privileged=False):
    """Check argv, cwd, open files and mapped files of the current user's tasks."""
    target = target.resolve()
    found = []
    inaccessible = False
    for proc in Path('/proc').iterdir():
        if not proc.name.isdigit() or int(proc.name) == os.getpid():
            continue
        try:
            if not privileged and proc.stat().st_uid != os.getuid():
                continue
            args = (proc / 'cmdline').read_bytes().decode(errors='replace').split('\0')
            if any(str(target) in arg for arg in args):
                found.append(int(proc.name))
            paths = [proc / 'cwd', proc / 'root', *list((proc / 'fd').iterdir())]
            for path in paths:
                try:
                    if path.resolve().is_relative_to(target):
                        found.append(int(proc.name))
                except FileNotFoundError:
                    pass
            if str(target) in (proc / 'maps').read_text(errors='replace'):
                found.append(int(proc.name))
        except (FileNotFoundError, ProcessLookupError):
            continue
        except PermissionError:
            inaccessible = True
            if privileged:
                found.append(int(proc.name))
    if inaccessible and not privileged:
        # Inspect protected session processes read-only; all file writes stay unprivileged.
        result = subprocess.run(['sudo', '-n', sys.executable, str(Path(__file__).resolve()),
                                 '--reference-check'], input=str(target), text=True,
                                stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True)
        found.extend(json.loads(result.stdout))
    return sorted(set(found))


def stamp(path):
    value = path.stat()
    return value.st_dev, value.st_ino, value.st_size, value.st_mtime_ns


def append_receipt(path, row):
    with path.open('a', encoding='utf-8') as stream:
        stream.write(json.dumps(row, sort_keys=True) + '\n')
        stream.flush()
        os.fsync(stream.fileno())


def seal(raw, scope, receipt, age, minimum, preserved=None, expected=None):
    if raw.is_symlink() or not raw.is_file():
        return False
    initial = stamp(raw)
    if initial[2] < minimum or time.time() - raw.stat().st_mtime < age:
        return False
    if process_references(scope):
        return False
    digest = sha(raw)
    if expected is not None and digest != expected:
        raise RuntimeError(f'Unexpected raw hash: {raw}')
    compressed = preserved or raw.with_name(raw.name + '.zst')
    created = not compressed.exists()
    if created:
        temporary = compressed.with_name(compressed.name + '.sealing')
        # Never overwrite a partial or completed preservation copy.
        with temporary.open('xb') as stream:
            subprocess.run(['zstd', '-q', '-1', '-T2', '-c', str(raw)], stdout=stream, check=True)
            stream.flush()
            os.fsync(stream.fileno())
        if decoded_sha(temporary) != digest:
            raise RuntimeError(f'Compression verification failed: {raw}')
        temporary.rename(compressed)
    elif decoded_sha(compressed) != digest:
        raise RuntimeError(f'Existing compressed copy differs: {raw}')
    if stamp(raw) != initial or process_references(scope):
        return False
    row = dict(raw=str(raw), compressed=str(compressed), raw_bytes=initial[2],
               compressed_bytes=compressed.stat().st_size, raw_sha256=digest,
               compressed_sha256=sha(compressed), two_process_checks_clear=True,
               decoded_bytes_verified=True, timestamp_utc=time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()))
    append_receipt(receipt, dict(row, action='verified_before_raw_unlink'))
    raw.unlink()
    append_receipt(receipt, dict(row, action='raw_removed'))
    print(json.dumps(dict(raw=str(raw), saved_bytes=initial[2] - (row['compressed_bytes'] if created else 0))), flush=True)
    return True


def sweep(args):
    jobs = args.jobs.resolve(strict=True)
    staging = args.staging.resolve(strict=True)
    receipt = jobs / 'vm-storage-maintenance.jsonl'
    before = shutil.disk_usage(jobs).free
    count = 0
    # Retained native diagnostics are preserved in place as compressed files.
    for raw in sorted(staging.glob('song-lua-*/Save/itgmania-harness/song-lua-trace.json'),
                      key=lambda path: path.stat().st_size, reverse=True):
        scope = raw.parents[2]
        if scope.parent != staging or scope.is_symlink():
            continue
        count += seal(raw, scope, receipt, args.min_age, args.min_bytes)
    # Terminal job metadata must exist before a report can be sealed.
    for raw in jobs.rglob('report.json'):
        if raw.is_symlink() or not raw.resolve().is_relative_to(jobs):
            continue
        scope = raw.parent
        if not (scope / 'result.json').is_file() and not (scope / 'proof.json').is_file():
            continue
        try:
            metadata = json.loads((scope / ('result.json' if (scope / 'result.json').exists() else 'proof.json')).read_bytes())
        except (OSError, ValueError):
            continue
        if metadata.get('status') == 'running':
            continue
        count += seal(raw, scope, receipt, args.min_age, args.min_bytes,
                      expected=metadata.get('report_sha256'))
    summary = dict(action='sweep_complete', files_sealed=count, free_before=before,
                   free_after=shutil.disk_usage(jobs).free, timestamp=time.time())
    append_receipt(receipt, summary)
    print(json.dumps(summary), flush=True)


def main():
    if sys.argv[1:] == ['--reference-check']:
        print(json.dumps(process_references(Path(sys.stdin.read()), privileged=True)))
        return
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--jobs', type=Path, default=Path('/home/parity/jobs'))
    parser.add_argument('--staging', type=Path, default=Path('/tmp/itgmania-harness-rs'))
    parser.add_argument('--min-age', type=int, default=600, help='Minimum seconds since raw file changed')
    parser.add_argument('--min-bytes', type=int, default=10_000_000)
    parser.add_argument('--watch', action='store_true', help='Repeat every five minutes; keeps receipts and skips active jobs')
    args = parser.parse_args()
    with (args.jobs / 'vm-storage-maintenance.lock').open('a') as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        while True:
            sweep(args)
            if not args.watch:
                break
            time.sleep(300)


if __name__ == '__main__':
    main()
