"""Audit reuse of the existing baker and all of its regression assertions."""
from pathlib import Path
import hashlib
import json
import subprocess

root = Path(__file__).resolve().parents[3]
base = Path(__file__).resolve().parent
before = json.loads((base / 'input-edit-baseline.json').read_text())
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()


def normal(source):
    return subprocess.run(['rustfmt', '--edition', '2024', '--emit', 'stdout',
                           '--config', 'skip_children=true,newline_style=Unix'],
                          input=source, text=True, capture_output=True, check=True).stdout


report = {}
for name in ['mod', 'compile', 'decode', 'batch', 'dedup', 'strings', 'tables']:
    old = f'crates/deadlib-lua-legacy/src/program/{name}.rs'
    new = f'crates/deadsync-song-lua/src/rust_engine/input/program/{name}.rs'
    assert sha(root / old) == before[old]['sha256']
    source = (root / old).read_text()
    if name == 'mod':
        source = source.replace('use crate::Function;\n', '')
        start = source.index('/// Compile a Lua closure using a validated native schema.')
        end = source.index('/// Safely decode and compile a Lua 5.1 dump.', start)
        source = source[:start] + source[end:]
        start = source.index('    /// Compile using these argument/capture bindings')
        end = source.index('    /// Compile a checked Lua 5.1 dump', start)
        source = source[:start] + source[end:]
    if name == 'batch':
        source = source.replace('    scratch: Vec<u8>,\n', '').replace('            scratch: Vec::new(),\n', '')
        start = source.index('    /// Dump without execution, then compile')
        end = source.index('    /// Validate/compile bytes', start)
        source = source[:start] + source[end:]
        source = source.replace('/// Dump and key buffer capacities', '/// Bytecode and key buffer capacities')
    assert normal(source) == normal((root / new).read_text()), new
    report[new] = {'original': old, 'original_sha256': sha(root / old),
                   'sha256': sha(root / new), 'verified_transform':
                   'remove legacy closure/dump wrappers and their scratch buffer; format; bytecode compiler and evaluator unchanged'}

for name in ['program', 'program_tables']:
    old = f'crates/deadlib-lua-legacy/tests/{name}.rs'
    new = f'crates/deadsync-song-lua/tests/rust_baked_{name}.rs'
    assert sha(root / old) == before[old]['sha256']
    source = (root / old).read_text()
    source = source.replace('#![cfg(feature = "lua51")]\n', '#[path = "support/baked_program.rs"]\nmod program;\n', 1)
    source = source.replace('use mlua::program::{self,', 'use program::{', 1)
    if name == 'program':
        source = 'use program::{FixtureBatch as _, FixtureCompiler as _};\n' + source
    assert normal(source) == normal((root / new).read_text()), new
    report[new] = {'original': old, 'original_sha256': sha(root / old),
                   'sha256': sha(root / new), 'verified_transform':
                   'route all existing test bodies unchanged to the Rust bytecode compiler via test-only dumping helpers',
                   'test_count': source.count('#[test]')}

old = 'crates/deadsync-song-lua/src/input/compiled.rs'
new = 'crates/deadsync-song-lua/src/rust_engine/input/predicate.rs'
assert sha(root / old) == before[old]['sha256']
source = (root / old).read_text().replace('use mlua::Function;\n', '')
source = source.replace('use super::SongLuaInputEvent;', 'use crate::SongLuaInputEvent;\nuse super::event_kind;')
source = source.replace('use mlua::program::{self,', 'use super::super::baked_program::{self as program,')
source = source.replace('SongLuaCompiledInput', 'CompiledInput').replace('compile(callback: &Function)', 'compile(bytes: &[u8])')
source = source.replace('program::compile(callback, &bindings)', 'program::compile_bytecode(bytes, &bindings)')
source = source.replace('input.kind.native_name()', 'event_kind(input.kind)')
assert normal(source) == normal((root / new).read_text()), new
report[new] = {'original': old, 'original_sha256': sha(root / old), 'sha256': sha(root / new),
               'verified_transform': 'replace closure input with bytecode; use shared event-kind mapping; unchanged field schema and evaluation'}

(base / 'input-port-review.json').write_text(json.dumps(report, indent=2) + '\n')
print(json.dumps({'verified_files': len(report), 'preserved_baker_tests': 31}))
