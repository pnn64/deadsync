"""Compare the pending GIF adapter with the exact frozen legacy capture code.

This isolated probe checks capture output before the application's backend is
changed. It is not a substitute for the real theme tests or memory-limit gate.
The two small enum shims below represent only the variants used by capture.
"""
from pathlib import Path
import hashlib, json, os, subprocess, time

root = Path(__file__).resolve().parents[3]
evidence = Path(__file__).resolve().parent
probe = root / 'target/gif-migration-probe'
(probe / 'src').mkdir(parents=True, exist_ok=True)
original = root / 'target/rust-lua-production-before/crates/deadsync-theme-simply-love/src/step_stats_gifs.rs'
source = original.read_text(encoding='utf-8')
start = source.index('struct GifStyle')
style = source[source.rfind('#[derive', 0, start):source.index('#[derive', start)]
start = source.index('struct CapturedGif')
captured = source[start:source.index('\n}', start)+2]
start = source.index('fn capture_script(')
legacy = source[start:source.index('#[cfg(test)]', start)]
for declaration in ['fn actor_command(', 'fn alignment_name(', 'fn numbered_key(', 'enum GifActorCommand']:
    legacy = legacy.replace(declaration, 'pub(super) ' + declaration)
prefix = '''#![allow(dead_code)]
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq)] enum EffectClock { Time, Beat }
#[derive(Clone, Copy)] enum PlayerSide { P1, P2 }
const MAX_FRAMES: usize = 4096;
const MAX_LUA_MEMORY: usize = 8 * 1024 * 1024;
const MAX_LUA_INSTRUCTIONS: u32 = 1_000_000;
const LUA_HOOK_INTERVAL: u32 = 10_000;
'''
adapter = (root / 'crates/deadsync-theme-simply-love/src/step_stats_gifs/rust_engine.rs').as_posix()
main = prefix + style + captured + '''
mod legacy {
use super::*;
use mlua::{HookTriggers, Lua, LuaOptions, MultiValue, StdLib, Table, Value, VmState};
pub(super) fn capture(s: &str, name: &str, side: PlayerSide, wide: bool) -> Result<CapturedGif, String> {
    capture_script(s, name, side, wide)
}
''' + legacy + '\n}\nuse legacy::{actor_command, alignment_name, numbered_key, GifActorCommand};\n'
main += '#[path = ' + json.dumps(adapter) + '] mod rust_engine;\n'
main += r'''
fn compare(source: &str, name: &str) {
    for side in [PlayerSide::P1, PlayerSide::P2] {
        for wide in [false, true] {
            let before = legacy::capture(source, name, side, wide);
            let mut vm = deadlib_lua::Vm::try_new_with_memory_limit(MAX_LUA_MEMORY).unwrap();
            let after = rust_engine::capture(&mut vm, source, name, side, wide);
            match (before, after) {
                (Ok(before), Ok(after)) => {
                    assert_eq!(before.texture, after.texture, "{name}: texture");
                    assert_eq!(before.frames, after.frames, "{name}: frames");
                    assert_eq!(before.delays.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                               after.delays.iter().map(|x| x.to_bits()).collect::<Vec<_>>(), "{name}: delays");
                    assert_eq!(before.style, after.style, "{name}: style");
                    for (a, b) in before.style.crop.into_iter().chain([before.style.x,before.style.y,before.style.zoom,before.style.align_x])
                        .zip(after.style.crop.into_iter().chain([after.style.x,after.style.y,after.style.zoom,after.style.align_x])) {
                        assert_eq!(a.to_bits(), b.to_bits(), "{name}: style bits");
                    }
                }
                (Err(_), Err(_)) => {},
                (Err(before), Ok(_)) => panic!("{name}: legacy rejected ({before}), Rust accepted"),
                (Ok(_), Err(after)) => panic!("{name}: legacy accepted, Rust rejected ({after})"),
            }
            vm.collect_garbage().unwrap();
            let usage = vm.memory_usage().unwrap();
            assert!(usage.bytes <= usage.peak_bytes && usage.peak_bytes <= MAX_LUA_MEMORY);
        }
    }
    println!("equal: {name}");
}
fn main() {
    let mut count = 0;
    for path in std::env::args().skip(1) {
        compare(&std::fs::read_to_string(&path).unwrap(), &path);
        count += 1;
    }
    for (name, source) in [
        ("ordering/coercion/GC", r#"local shared = 0; return Def.Sprite {Texture='x', frame0='3', frame2=7, delay0='0.1', InitCommand=function(self) shared=6; self:x('1.2'):addx(0.1):halign('RiGhT'); collectgarbage('collect') end, OnCommand=function(self) assert(shared==6); self:xy(0/0, '-0'):zoom('nan'):unknown():EffectClock('BgM'); collectgarbage('collect') end}"#),
        ("limits/linear", "return Def.Sprite { Texture='x', Frames=Sprite.LinearFrames(5000, 0.2) }"),
        ("linear/coercion", "return Def.Sprite { Texture='x', Frames=Sprite.LinearFrames('2.9', '0.03') }"),
        ("linear/negative-fraction", "return Def.Sprite { Texture='x', Frames=Sprite.LinearFrames(-0.5, 0.03) }"),
        ("linear/manual-hex", "return Def.Sprite { Texture='x', Frames={__deadsync_count='0x2', __deadsync_seconds='0x1'} }"),
        ("linear/rounding", "return Def.Sprite { Texture='x', Frames={__deadsync_count=1, __deadsync_seconds=1.000000059604645} }"),
        ("numeric/frame-overflow", "return Def.Sprite { Texture='x', frame0=4294967296, frame1=4294967296.5, frame2=-0.0 }"),
        ("callback/GC/reentry", "return setmetatable({Texture='x', OnCommand=function(self) collectgarbage('collect'); self:addx(2) end}, {__index=function(self,k) collectgarbage('collect'); if k=='__deadsync_sprite' then return true elseif k=='InitCommand' then return function(self) self:addx(3) end end end})"),
        ("sandbox", "assert(os==nil and io==nil and package==nil and require==nil and debug==nil and coroutine~=nil); return Def.Sprite{Texture='x'}"),
        ("no sprite", "return {}"),
        ("two sprites", "return {Def.Sprite{Texture='x'}, Def.Sprite{Texture='y'}}"),
        ("frame limit", "return Def.Sprite {Texture='x', frame4096=0}"),
        ("runaway root", "while true do end"),
        ("runaway callback", "return Def.Sprite {Texture='x', OnCommand=function() while true do end end}"),
        ("oversized runtime string", "return Def.Sprite {Texture=string.rep('x', 16*1024*1024)}"),
        ("oversized runtime table", "local t={} for i=1,300000 do t[i]={i} end return Def.Sprite{Texture='x'}"),
    ] {
        compare(source, name);
        count += 1;
    }
    println!("{count} cases, {} player/aspect comparisons", count * 4);
}
'''
(probe / 'src/main.rs').write_text(main, encoding='utf-8')
(probe / 'Cargo.toml').write_text('''[package]
name = "gif-migration-probe"
version = "0.0.0"
edition = "2024"
[workspace]
[dependencies]
deadlib-lua = { path = "../../crates/deadlib-lua" }
mlua = { package = "deadlib-lua-legacy", path = "../../crates/deadlib-lua-legacy", features = ["lua51", "vendored"] }
[patch.crates-io]
lua-src = { path = "../../vendor/lua-src" }
''')
files = json.loads((evidence / 'baseline-sources.json').read_text())
fixtures = [str(root / name) for name in files if 'step_stats_gifs' in name and name.endswith('.lua')]
fixture_hashes = {str(Path(path).relative_to(root)): hashlib.sha256(Path(path).read_bytes()).hexdigest() for path in fixtures}
assert all(digest == files[Path(name).as_posix()] for name, digest in fixture_hashes.items())
env = os.environ.copy()
env.update(CARGO_TARGET_DIR=str(root / 'target/gif-migration-probe-target') if os.name == 'nt' else '/var/tmp/deadsync-gif-migration-target', CARGO_PROFILE_DEV_DEBUG='0', CARGO_INCREMENTAL='0')

def engine_sources():
    names = subprocess.check_output(['git', 'ls-files', '--cached', '--others',
        '--exclude-standard', '-z', '--', 'crates/deadlib-lua'], cwd=root).decode('utf-8').split('\0')
    return {name.replace('\\', '/'): hashlib.sha256((root / name).read_bytes()).hexdigest()
            for name in sorted(set(names)) if name and (root / name).is_file()
            and Path(name).suffix in {'.rs', '.toml', '.lua'}}

engine_before = engine_sources()
command = ['cargo', 'run', '--manifest-path', str(probe / 'Cargo.toml'), '--', *fixtures]
started = time.monotonic()
run = evidence / 'gif-probe-runs' / f'{time.time_ns()}'
run.mkdir(parents=True, exist_ok=False)
(run / 'engine-sources.json').write_text(json.dumps(engine_before, indent=2)+'\n')
log = run / 'run.log'
adapter_hash = hashlib.sha256(Path(adapter).read_bytes()).hexdigest()
with log.open('wb') as stream:
    result = subprocess.run(command, cwd=root, env=env, stdout=stream, stderr=subprocess.STDOUT)
report = dict(command=command, exit_code=result.returncode, seconds=time.monotonic()-started,
              adapter_sha256=adapter_hash,
              sources_unchanged=(adapter_hash == hashlib.sha256(Path(adapter).read_bytes()).hexdigest()
                                 and engine_before == engine_sources()),
              engine_sources_sha256=hashlib.sha256((run / 'engine-sources.json').read_bytes()).hexdigest(),
              legacy_sha256=hashlib.sha256(original.read_bytes()).hexdigest(),
              fixture_sha256=fixture_hashes,
              log_sha256=hashlib.sha256(log.read_bytes()).hexdigest(),
              scope='isolated capture differential using bounded VMs, including runtime quota rejection; not application integration or exhaustive memory validation')
(run / 'result.json').write_text(json.dumps(report, indent=2)+'\n')
print(log.read_text(errors='replace')[-6000:], flush=True)
assert report['sources_unchanged']
raise SystemExit(result.returncode)
