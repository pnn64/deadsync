"""One-time noteskin cutover, preserving the previous compiler as a dev oracle."""
from pathlib import Path
import hashlib, json

root = Path(__file__).resolve().parents[3]
crate = root / 'crates/deadsync-noteskin'
compiler = crate / 'src/compiler.rs'
old = compiler.read_text()
assert 'const COMPILER_VERSION: u32 = 29;' in old
assert 'use mlua::' in old
support = crate / 'tests/support'
support.mkdir(exist_ok=True)
oracle = support / 'legacy.rs'
assert not oracle.exists()
frozen = old[:old.index('#[cfg(test)]\nmod tests {')]
frozen = frozen.replace('#[cfg(feature = "rust-engine")]\nmod rust_engine;\n#[cfg(feature = "rust-engine")]\npub use rust_engine::compile_data_rust;\n', '')
oracle.write_text('// Frozen production compiler before the Rust engine cutover.\n'
                  '// Compiled only by tests/examples through dev-dependencies.\n' + frozen)
(Path(__file__).parent / 'noteskin-oracle-source.json').write_text(json.dumps({
    'original_sha256': hashlib.sha256(old.encode()).hexdigest(),
    'oracle_sha256': hashlib.sha256(oracle.read_bytes()).hexdigest(),
    'transformations': ['strip inactive rust-engine module/re-export',
                        'exclude original inline tests (retained in production compiler)',
                        'add provenance comment'],
    'compiler_version': 29,
}, indent=2) + '\n')

current = old.replace('#[cfg(feature = "rust-engine")]\n', '')
current = current.replace('pub use rust_engine::compile_data_rust;', 'pub use rust_engine::compile_data;')
current = current.replace('use mlua::{Function, IgnoredArgs, Lua, MultiValue, Table, Value};\n', '')
current = current.replace('const COMPILER_VERSION: u32 = 29;', 'const COMPILER_VERSION: u32 = 30;')
for start, end in [
    ('/// Compile an isolated skin', 'fn noteskin_paths('),
    ('fn compile_actor_files(', 'fn compile_actor_files_with('),
    ('fn compile_color_paths(', 'fn path_is_lua('),
    ('fn append_actor_command(', '#[derive(Debug, Eq, PartialEq)]\nstruct LoaderSortKey'),
    ('fn normalize_noteskin_tables(', 'fn game_buttons('),
    ('fn actor_loader_command(', '#[cfg(test)]\nmod tests {'),
]:
    first, last = current.index(start), current.index(end)
    assert first < last
    current = current[:first] + current[last:]
current = current[:current.index('#[cfg(test)]\n#[path = "compiler_callbacks_perf.rs"]')]
current += '#[cfg(test)]\n#[allow(dead_code)]\n#[path = "compiler/legacy_tests.rs"]\nmod legacy;\n'
compiler.write_text(current)
(crate / 'src/compiler/legacy_tests.rs').write_text(
    '// Historical optimization controls remain isolated from production.\n'
    'include!("../../tests/support/legacy.rs");\n\n'
    '#[path = "../compiler_callbacks_perf.rs"]\nmod callbacks_perf;\n\n'
    '#[path = "../compiler_aliases_perf.rs"]\nmod aliases_perf;\n')

adapter = crate / 'src/compiler/rust_engine.rs'
text = adapter.read_text().replace('//! Opt-in noteskin preparation using the independent Rust Lua engine.',
    '//! Production noteskin preparation using the shared Rust Lua engine.')
start = text.index('/// Compile the same native noteskin bundle')
end = text.index('pub fn compile_data_rust(', start)
text = text[:start] + ('/// Compile an isolated skin without consulting or populating the session cache.\n'
                      '/// An empty source hash is suitable for a song-lifetime, in-memory bundle.\n') + text[end:]
text = text.replace('pub fn compile_data_rust(', 'pub fn compile_data(')
adapter.write_text(text)

manifest = crate / 'Cargo.toml'
text = manifest.read_text().replace('[features]\nrust-engine = ["dep:deadlib-lua"]\n\n', '')
text = text.replace(', optional = true', '')
text = text.replace('mlua = { workspace = true, features = ["lua51", "vendored"] }\n', '')
text = text.replace('[lints.clippy]', '[dev-dependencies]\nmlua = { workspace = true, features = ["lua51", "vendored"] }\n\n[lints.clippy]')
text = text.replace('required-features = ["rust-engine"]\n', '')
manifest.write_text(text)

for relative in ['tests/rust_engine.rs', 'examples/compare_lua_engines.rs']:
    path = crate / relative
    text = path.read_text().replace('#![cfg(feature = "rust-engine")]\n\n', '')
    # Root aliases let the exact frozen compiler use its original crate paths.
    if relative.startswith('tests/'):
        text = text.replace('use deadsync_noteskin::{compiled, compiler, itg};',
            'pub use deadsync_noteskin::{Quantization, actor, compiled, itg};\nuse deadsync_noteskin::compiler;\n\n'
            '#[allow(dead_code)]\n#[path = "support/legacy.rs"]\nmod legacy;')
    else:
        text = text.replace('use deadsync_noteskin::{compiled::*, compiler, itg};',
            'pub use deadsync_noteskin::{Quantization, actor, compiled, itg};\nuse deadsync_noteskin::{compiled::*, compiler};\n\n'
            '#[allow(dead_code)]\n#[path = "../tests/support/legacy.rs"]\nmod legacy;')
    text = text.replace('compiler::compile_data(', 'legacy::compile_data(')
    text = text.replace('compiler::compile_data_rust(', 'compiler::compile_data(')
    path.write_text(text)
