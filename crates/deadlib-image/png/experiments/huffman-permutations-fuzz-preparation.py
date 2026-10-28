from pathlib import Path
import hashlib, json, os, shutil, subprocess, time, zipfile

root = Path('/mnt/c/GitHub/deadsync')
png = root / 'crates/deadlib-image/png'
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()

for name in ['stored-growth', 'stream-copy']:
    campaign = Path('/tmp') / f'deadlib-image-{name}-asan-20261027-a'
    source = campaign / 'corpus'
    destination = root / f'target/png-fuzz-corpus-{name}-20261028.zip'
    report = png / f'fuzz/reports/huffman-permutations-{name}-corpus-storage.json'
    prior = png / f'fuzz/reports/{name}-source.json'
    assert source.resolve() == source and source.parent == campaign
    assert campaign.parent == Path('/tmp') and source.is_dir()
    assert not destination.exists() and not report.exists()
    metadata = json.loads(prior.read_text())
    assert metadata['isolation_verified'] and metadata['final_source_hashes_verified']
    assert all(c['exit_code'] == 0 for c in metadata['commands'])
    binaries = {c['binary'].encode() for c in metadata['campaigns']}
    for process in Path('/proc').iterdir():
        if not process.name.isdigit():
            continue
        try:
            args = (process / 'cmdline').read_bytes().split(b'\0')
        except (FileNotFoundError, PermissionError, ProcessLookupError):
            continue
        assert not binaries.intersection(args), ('Live corpus owner', process)
    before = os.statvfs('/tmp')
    paths = list(source.rglob('*'))
    assert not any(p.is_symlink() for p in paths)
    files = sorted([p for p in paths if p.is_file()], key=lambda p: p.relative_to(source).as_posix().encode())
    assert len(files) >= 48167 + 6927
    digest = hashlib.sha256(); total = 0; start = time.monotonic()
    with zipfile.ZipFile(destination, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=1) as archive:
        for path in files:
            relative = path.relative_to(source).as_posix(); payload = path.read_bytes()
            entry = zipfile.ZipInfo(relative, (1980, 1, 1, 0, 0, 0))
            entry.create_system = 3; entry.external_attr = 0o100644 << 16
            archive.writestr(entry, payload, compress_type=zipfile.ZIP_DEFLATED, compresslevel=1)
            digest.update(relative.encode() + b'\0' + payload); total += len(payload)
    verified = hashlib.sha256()
    with zipfile.ZipFile(destination) as archive:
        assert archive.namelist() == [p.relative_to(source).as_posix() for p in files]
        for path in files:
            relative = path.relative_to(source).as_posix(); payload = archive.read(relative)
            assert payload == path.read_bytes(), relative
            verified.update(relative.encode() + b'\0' + payload)
    assert verified.digest() == digest.digest()
    data = dict(reason='Relocate completed mutable corpora to free tmpfs inodes; preserve all binaries, logs and build audits.', source=str(source),
                prior_evidence=prior.relative_to(png).as_posix(), prior_evidence_sha256=sha(prior),
                archive=destination.relative_to(root).as_posix(), archive_sha256=sha(destination), archive_bytes=destination.stat().st_size,
                files=len(files), source_bytes=total, tree_sha256=digest.hexdigest(),
                hash_method='Ascending literal UTF-8 relative filename bytes, NUL, contents',
                all_archive_entries_match_original=True, before_free_inodes=before.f_favail, runner_sha256=sha(Path(__file__)))
    report.write_text(json.dumps(data, indent=2) + '\n')
    assert source.resolve() == Path('/tmp') / f'deadlib-image-{name}-asan-20261027-a/corpus'
    shutil.rmtree(source)
    data.update(temporary_corpus_removed_after_verification=True, after_free_inodes=os.statvfs('/tmp').f_favail,
                wall_seconds=round(time.monotonic() - start, 3))
    report.write_text(json.dumps(data, indent=2) + '\n')
    print(json.dumps(data), flush=True)

base = Path('/tmp/deadlib-image-stream-literals-seeds-20261027-a.zip')
assert sha(base) == 'aa9f00512df5de65ec0e3d62463e8072a83d69396ca21307010128153ae2cd6b'
export = root / 'target/png-huffman-permutations-seeds-20261027-a'
for target in ['png_stream', 'inflate_stream']:
    assert len(list((export / target).iterdir())) == 10744
archive = Path('/tmp/deadlib-image-huffman-permutations-seeds-20261028-a.zip')
subprocess.run(['python3', str(png / 'fuzz/scripts/archive_native_seeds.py'), '--corpus-root', str(export),
                '--output', str(archive), '--targets', 'png_stream', 'inflate_stream', '--include-archive', str(base)], check=True)
description = json.loads(archive.with_suffix('.json').read_text())
assert {r['target']: r['files'] for r in description['initial_corpora']} == {'png_stream': 58911, 'inflate_stream': 17671}
assert os.statvfs('/tmp').f_favail > 100000
print('Fresh merged archive verified; sufficient free inodes.', flush=True)
