"""Remove the engine switch and keep paired regression controls on native Lua."""
import hashlib
import json
import re
from pathlib import Path
d = Path(__file__).resolve().parent
r = d.parents[2]
song = r / 'crates/deadsync-song-lua'
saved = json.loads((d / 'production-song-cutover-edit-baseline.json').read_text())
out = d / 'production-song-routing-wiring.json'
assert not out.exists()
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
manifest = song / 'Cargo.toml'
assert sha(manifest) == saved[manifest.relative_to(r).as_posix()]['sha256']
text = manifest.read_text()
assert text.count('rust-engine = ["dep:deadlib-lua"]') == 1
text = text.replace('rust-engine = ["dep:deadlib-lua"]\n', '')
assert text.count('optional = true, ') == 1
text = text.replace('optional = true, ', '')
text = text.replace('["rust-engine", "test-support"]', '["test-support"]')
text = text.replace('required-features = ["rust-engine"]\n', '')
assert 'rust-engine' not in text
manifest.write_text(text)
names = ['compile_song_lua_layers_with_default_host', 'compile_song_lua_with_default_host',
         'reload_song_lua_with_default_host', 'SongLuaLiveScene']
qualified = re.compile(r'\b(song|deadsync_song_lua)::(' + '|'.join(names + ['CompiledSongLua']) + r')\b')
rows = []
for path in sorted((song / 'tests').glob('rust_*.rs')):
    if path.name == 'rust_actor_input_contract.rs':
        continue  # This existing contract deliberately selects the Rust scene.
    before = path.read_text()
    text, count = qualified.subn(r'\1::legacy::\2', before)
    def imports(match):
        value = match[0]
        for name in names:
            value = re.sub(r'(?<![:\w])' + name + r'\b', 'legacy::' + name, value)
        return value
    text = re.sub(r'use deadsync_song_lua::\{.*?\};', imports, text, flags=re.S)
    if path.name == 'rust_actor_inputs.rs':
        text = text.replace('use deadsync_song_lua::{self as song, rust_engine as rust, *};',
                            'use deadsync_song_lua::{self as song, legacy::SongLuaLiveScene, rust_engine as rust, *};')
    if text != before:
        assert sha(path) == saved[path.relative_to(r).as_posix()]['sha256']
        path.write_text(text)
        rows.append(dict(path=path.relative_to(r).as_posix(), qualified_sites=count))
out.write_text(json.dumps({'manifest': manifest.relative_to(r).as_posix(),
                           'explicit_native_controls': rows}, indent=2) + '\n')
print(f'Removed the engine feature and redirected native controls in {len(rows)} regression files.', flush=True)
