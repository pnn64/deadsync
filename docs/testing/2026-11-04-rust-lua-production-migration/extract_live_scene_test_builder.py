"""Reuse the unchanged Rust scene setup for the explicit runtime handoff check."""
from pathlib import Path
d=Path(__file__).parent;r=d.resolve().parents[2]
p=r/'crates/deadsync-song-lua/tests/rust_live_scene.rs'
s=p.read_text();archive=d/'song-live-scene-tests-before-handoff.rs';assert not archive.exists();archive.write_bytes(p.read_bytes())
a=s.index('        let mut rust = RustSongRuntime::new(&context).unwrap();')
b=s.index('        let mut pair = Self {',a)
block=s[a:b];body='\n'.join(line[4:] if line.startswith('    ') else line for line in block.splitlines())
fn='fn rust_scene(context: &SongLuaCompileContext, paths: &[&Path]) -> SongLiveScene {\n'+body+'\n    rust\n}\n'
s=s[:a]+'        let rust = rust_scene(&context, &paths);\n'+s[b:]
idx=s.index('impl Pair {');s=s[:idx]+fn+s[idx:];p.write_text(s)
print('extracted existing Rust scene construction; all checks preserved')
