"""Execute original test targets already built from the frozen Lua source.

Four targets not rebuilt before Cargo's example failure run separately through
production-current-remaining-test-targets-v4. Their coverage remains pending.
"""
from pathlib import Path
import hashlib, json, re, shutil, subprocess, time

d = Path(__file__).resolve().parent
r = d.parents[2]
out = d/'production-current-built-regressions-v3'
out.mkdir(exist_ok=False)
inventory = json.loads((d/'production-current-regression-target-inventory-v1.json').read_text())
build = d/'production-current-full-song-v2-song-windows-debug'
build_sources = json.loads((build/'sources.json').read_text())
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()

def sources():
    scope = ['deadlib-lua','deadsync-song-lua','deadsync-noteskin','deadsync-theme-simply-love',
             'deadsync-assets','deadsync-shell','deadsync-notefield']
    names = subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z','--',
        *['crates/'+name for name in scope],'Cargo.toml','Cargo.lock','.cargo/config.toml'],cwd=r).decode().split('\0')
    return {n:sha(r/n) for n in sorted(set(names)) if n and (r/n).is_file()
        and (Path(n).suffix in {'.rs','.lua','.toml'} or n=='Cargo.lock')}

before = sources()
changes = [n for n in sorted(before.keys()|build_sources.keys()) if before.get(n)!=build_sources.get(n)]
allowed = {'crates/deadsync-notefield/src/transforms.rs',
           'crates/deadsync-notefield/src/appearance_performance.rs',
           'crates/deadsync-song-lua/examples/input_frames.rs'}
assert set(changes) <= allowed, changes
proof = json.loads((d/'production-concurrent-notefield-source-review-v1/review.json').read_text())
assert before['crates/deadsync-notefield/src/transforms.rs'] == proof['after_sha256']
assert before['crates/deadsync-notefield/src/appearance_performance.rs'] == proof['new_test_file_sha256']
old_log = (d/'production-final-full-song-v1-song-windows-debug/run.log').read_text(errors='replace')
original = set(re.findall(r'Running tests\\([^\s\\]+)\.rs',old_log))|{'deadsync_song_lua'}
assert original == {row['target']['name'] for row in inventory['targets']}
(out/'sources.json').write_text(json.dumps(before,indent=2)+'\n')

fresh = [row for row in inventory['targets'] if row['candidates'][0]['newer_than_full_build_snapshot']]
pending = [row['target']['name'] for row in inventory['targets'] if not row['candidates'][0]['newer_than_full_build_snapshot']]
assert set(pending)=={'rust_live_layers','rust_live_visuals','rust_render','rust_statistics'}
plan = dict(original_eligible_targets=len(original),executing=len(fresh),pending_cargo_targets=pending,
    build_stage=str(build.relative_to(r)),build_to_execution_source_changes=changes,
    qualification='Only two independently reviewed notefield changes and the development-only input_frames module import differ. Runtime/library/test Lua source is exact. Pure notefield identity equivalence in production-concurrent-notefield-source-review-v1; added benchmark and example are not compiled as dependencies/test targets.',
    no_test_filters=True,original_ignore_attributes_unchanged=True)
(out/'plan.json').write_text(json.dumps(plan,indent=2)+'\n')
print(json.dumps(plan),flush=True)
rows=[]
base=(r/'target/rust-adapters/debug/deps').resolve()
for index,item in enumerate(fresh):
    target=item['target']['name']
    binary=Path(item['candidates'][0]['path']).resolve()
    assert binary.parent==base and binary.is_file()
    digest=sha(binary)
    log=out/(target+'.log')
    cmd=[str(binary),'--nocapture','--test-threads=1']
    started=time.monotonic()
    launch_error=None
    copied=None
    try:
        with log.open('wb') as f: result=subprocess.run(cmd,cwd=r,stdout=f,stderr=subprocess.STDOUT)
        code=result.returncode
    except OSError as error:
        launch_error=dict(winerror=error.winerror,message=str(error),original_command=cmd)
        if error.winerror==740:
            copied=base/f'migration-control-current-binaries-v3-{index}.exe'
            assert not copied.exists()
            shutil.copyfile(binary,copied)
            assert sha(copied)==digest
            cmd=[str(copied),'--nocapture','--test-threads=1']
            with log.open('wb') as f: result=subprocess.run(cmd,cwd=r,stdout=f,stderr=subprocess.STDOUT)
            code=result.returncode
        else:
            code=None
    text=log.read_text(errors='replace')
    results=re.findall(r'test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored;',text)
    row=dict(target=target,original_binary=str(binary),binary_sha256=digest,binary_unchanged=sha(binary)==digest,
        fingerprint=item['candidates'][0]['fingerprint'],command=cmd,exit_code=code,seconds=time.monotonic()-started,
        tests_passed=sum(int(x[0]) for x in results),tests_failed=sum(int(x[1]) for x in results),
        tests_ignored=sum(int(x[2]) for x in results),log_sha256=sha(log),launch_error=launch_error,
        neutral_copy_sha256=sha(copied) if copied else None,elevation_used=False)
    rows.append(row)
    (out/'targets.json').write_text(json.dumps(rows,indent=2)+'\n')
    print(json.dumps({k:row[k] for k in ['target','exit_code','tests_passed','tests_failed','tests_ignored']}),flush=True)
after=sources()
report=dict(executed_targets=len(rows),expected_executed_targets=len(fresh),original_eligible_targets=len(original),
    pending_cargo_targets=pending,all_original_targets_covered=False,
    tests_passed=sum(row['tests_passed'] for row in rows),tests_failed=sum(row['tests_failed'] for row in rows),
    tests_ignored=sum(row['tests_ignored'] for row in rows),
    failed_targets=[row['target'] for row in rows if row['exit_code']!=0],
    sources_unchanged=before==after,binaries_unchanged=all(row['binary_unchanged'] for row in rows),
    build_to_execution_source_changes=changes,
    launch_retries=sum(row['launch_error'] is not None for row in rows),elevation_used=False)
(out/'result.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report),flush=True)
assert report['sources_unchanged'] and report['binaries_unchanged']
raise SystemExit(1 if report['failed_targets'] else 0)
