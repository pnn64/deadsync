from pathlib import Path
import hashlib, json, os, shutil, zipfile

root = Path('/mnt/c/GitHub/deadsync'); png = root / 'crates/deadlib-image/png'
output = Path('/tmp/deadlib-image-huffman-permutations-asan-20261028-a')
target = Path('/tmp/deadlib-image-huffman-permutations-asan-target-20261028-a')
source = output / 'corpus'
archive = Path('/tmp/deadlib-image-huffman-permutations-seeds-20261028-a.zip')
report = png / 'fuzz/reports/huffman-permutations-launch-recovery.json'
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
assert not report.exists()
assert source.resolve() == Path('/tmp/deadlib-image-huffman-permutations-asan-20261028-a/corpus')
assert source.is_dir() and not (output / 'manifest.json').exists()
assert not list(output.glob('*.log')) and list(output.iterdir())
assert target.is_dir() and not list(target.iterdir())
for process in Path('/proc').iterdir():
    if not process.name.isdigit():
        continue
    try:
        args = (process / 'cmdline').read_bytes().split(b'\0')
    except (FileNotFoundError, PermissionError, ProcessLookupError):
        continue
    assert not any(arg.startswith(str(target).encode() + b'/') for arg in args), ('Live target', process)
paths = list(source.rglob('*'))
assert not any(p.is_symlink() for p in paths)
files = sorted(p.relative_to(source).as_posix() for p in paths if p.is_file())
assert len(files) == 76582
with zipfile.ZipFile(archive) as seeds:
    assert sorted(seeds.namelist()) == files
    for entry in seeds.infolist():
        assert seeds.read(entry) == (source / entry.filename).read_bytes(), entry.filename
data = dict(reason='Direct WSL Python launch could not find rustup; terminal exec session 10840 exited 1 in rustc-version discovery before manifest creation or compilation. Retry through the established login-shell environment.',
            failed_output=str(output), failed_target=str(target), failed_exec_session=10840, failed_exit_code=1,
            compiled_artifacts=0, fuzz_targets_run=0, all_extracted_seed_bytes_verified=True, seed_files=len(files),
            seed_archive=str(archive), seed_archive_sha256=sha(archive), wrapper_sha256=sha(output / 'native_rustc.py'),
            retained_artifacts=[p.relative_to(output).as_posix() for p in output.iterdir() if p.is_file()],
            recovery_runner_sha256=sha(Path(__file__)), before_free_inodes=os.statvfs('/tmp').f_favail)
report.write_text(json.dumps(data, indent=2) + '\n')
assert source.resolve() == Path('/tmp/deadlib-image-huffman-permutations-asan-20261028-a/corpus')
shutil.rmtree(source)
data.update(duplicate_seed_extraction_removed_after_verification=True, after_free_inodes=os.statvfs('/tmp').f_favail)
report.write_text(json.dumps(data, indent=2) + '\n')
print(json.dumps(data), flush=True)
