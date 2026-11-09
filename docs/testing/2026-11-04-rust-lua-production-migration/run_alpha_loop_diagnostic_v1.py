"""Compare the existing alpha-loop preparation tracks without changing its test."""
from pathlib import Path
import hashlib, json, os, subprocess, time

ledger = Path(__file__).resolve().parent
root = ledger.parents[2]
out = ledger / 'production-alpha-loop-diagnostic-v1'
out.mkdir(exist_ok=False)
(out / 'src').mkdir()
original = ledger / 'production-sharkmode-diagnostic-v1'
manifest = (original / 'Cargo.toml').read_text().replace('production-sharkmode-control', 'production-alpha-loop-control')
manifest = manifest.replace('[dependencies]', '[dependencies]\ndeadsync-rules = { path = "C:/GitHub/deadsync/crates/deadsync-rules" }')
(out / 'Cargo.toml').write_text(manifest)
(out / 'Cargo.lock').write_bytes((root / 'Cargo.lock').read_bytes())
source = (original / 'src/main.rs').read_text()
start, end = source.index('fn summary<'), source.index('fn main()')
source = source[:start] + '''fn summary<A: std::fmt::Debug, S>(value: deadsync_song_lua::CompiledSongLuaData<A, S>) -> serde_json::Value {
    json!({"overlays":format!("{:#?}",value.overlays),
        "updates":format!("{:#?}",value.overlay_updates),
        "eases":format!("{:#?}",value.overlay_eases),
        "messages":format!("{:#?}",value.stateful_message_captures),
        "info":format!("{:?}",value.info),"live":value.live_scene.is_some()})
}
''' + source[end:]
start, end = source.index('    let dir = out'), source.index('    let mut context =')
source = source[:start] + '''    let dir = root.join("tests/fixtures/itgmania-song-lua-micro/queued-alpha-loop");
    let path = dir.join("control.lua");
''' + source[end:]
source = source.replace('existing Sharkmode initialization diagnostic', 'existing alpha-loop preparation diagnostic')
source = source.replace('    context.screen_width = 854.;', '''    context.song_timing = Some(deadsync_rules::timing::TimingData::from_segments(0.,0.,
        &deadsync_rules::timing::TimingSegments{bpms:vec![(0.,60.)],
            warps:vec![deadsync_rules::timing::WarpSegment{beat:2.5,length:16.}],
            ..Default::default()},&[]));
    context.screen_width = 854.;''')
(out / 'src/main.rs').write_text(source)
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
names = json.loads((ledger / 'production-original-semantic-regressions-v1-downstream-windows-debug/sources.json').read_text())
before = {name: sha(root / name) for name in names}
helper = {str(p.relative_to(root)): sha(p) for p in [out / 'Cargo.toml', out / 'src/main.rs']}
(out / 'sources.json').write_text(json.dumps(dict(runtime=before, helper=helper), indent=2)+'\n')
env = os.environ.copy()
env.update(CARGO_TARGET_DIR=str(root / 'target/rust-adapters'), CARGO_INCREMENTAL='0', CARGO_PROFILE_DEV_DEBUG='0', CARGO_BUILD_JOBS='2')
command = ['cargo','run','--offline','--manifest-path',str(out / 'Cargo.toml'),'--bin','production-alpha-loop-control']
started = time.monotonic()
with (out / 'run.log').open('wb') as stream:
    result = subprocess.run(command, cwd=root, env=env, stdout=stream, stderr=subprocess.STDOUT)
changed = [name for name, digest in {**before, **helper}.items() if sha(root / name) != digest]
report = dict(command=command, exit_code=result.returncode, seconds=time.monotonic()-started, sources_unchanged=not changed,
    changed_sources=changed, log_sha256=sha(out / 'run.log'),
    limit='Existing alpha-loop four-second BPM60/warp preparation track diagnosis; no model asset loading or original root regression acceptance claim')
(out / 'result.json').write_text(json.dumps(report, indent=2)+'\n')
print(json.dumps(report), flush=True)
assert not changed
raise SystemExit(result.returncode)
