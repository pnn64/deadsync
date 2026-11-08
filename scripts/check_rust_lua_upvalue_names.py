"""Compare stripped/partial native upvalue names with official Lua 5.1.

The official compiler produces the input. Only debug name strings/counts and
optional location fields are rewritten; code, constants and captures stay intact.
Rust direct, persistent-cache store and fresh-process hit must match the C CLI.
This is a bounded compatibility audit, not full Lua/C API conformance.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
from build_lua_cli_reference import build


def rewrite(data, names=None, style='original', strip=False):
    """Decode the fixed valid fixture, preserving unrelated native wire bytes."""
    assert data[:6] == b'\x1bLua\x51\0'
    order = 'little' if data[6] else 'big'
    assert data[7] == data[9] == 4 and data[10:] and data[10:12] == b'\x08\0'
    width = data[8]
    assert width in (4, 8)
    position = 12
    targets = 0

    def take(size):
        nonlocal position
        result = data[position:position+size]
        assert len(result) == size
        position += size
        return result

    def integer(number):
        return number.to_bytes(4, order)

    def count():
        number = int.from_bytes(take(4), order)
        assert number <= len(data)
        return number

    def string():
        size = take(width)
        value = take(int.from_bytes(size, order))
        assert not value or value[-1:] == b'\0'
        return size+value

    def text(value):
        return (len(value)+1).to_bytes(width, order)+value+b'\0'

    def vector(size):
        length = count()
        return integer(length)+take(size*length)

    def function(depth):
        nonlocal targets
        assert depth < 20
        source = string()
        defined = take(8)
        flags = take(4)
        code = vector(4)
        constants = [integer(count())]
        for _ in range(int.from_bytes(constants[0], order)):
            tag = take(1)
            constants.append(tag)
            if tag == b'\x01': constants.append(take(1))
            elif tag == b'\x03': constants.append(take(8))
            elif tag == b'\x04': constants.append(string())
            else: assert tag == b'\0'
        child_count = count()
        children = [function(depth+1) for _ in range(child_count)]
        lines = vector(4)
        local_count = count()
        locals_ = [string()+take(8) for _ in range(local_count)]
        name_count = count()
        upvalues = [string() for _ in range(name_count)]
        if flags[0] == 2:
            assert name_count == 2
            targets += 1
            if names is not None:
                upvalues = upvalues[:names]
                if style == 'empty': upvalues = [text(b'') for _ in upvalues]
                elif style == 'nul': upvalues = [text(b'name\0suffix') for _ in upvalues]
                else: assert style == 'original'
        if strip:
            source = bytes(width)
            lines = integer(0)
            locals_ = []
        return b''.join([source, defined, flags, code, *constants,
                         integer(child_count), *children, lines,
                         integer(len(locals_)), *locals_, integer(len(upvalues)), *upvalues])

    result = data[:12]+function(0)
    assert position == len(data) and targets >= 3
    return result, targets


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--target', choices=['i686-unknown-linux-gnu'])
    parser.add_argument('--reference-fp-model', choices=['sse2'])
    parser.add_argument('--run-root', type=Path)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    fixture = root/'crates/deadlib-lua/tests/upvalue-names.lua'
    files = list((root/'crates/deadlib-lua').rglob('*.rs'))
    files += [fixture, root/'crates/deadlib-lua/Cargo.toml', Path(__file__).resolve(), root/'scripts/build_lua_cli_reference.py']
    sha = lambda p: hashlib.sha256(Path(p).read_bytes()).hexdigest()
    hashes = lambda: {p.relative_to(root).as_posix(): sha(p) for p in sorted(files)}
    before = hashes()
    command = ['cargo', 'build', '--release', '-p', 'deadlib-lua', '--bin', 'deadlua', '--example', 'os_process']
    if args.target: command += ['--target', args.target]
    subprocess.run(command, cwd=root, check=True)
    reference, metadata = build(root, args.target, args.reference_fp_model)
    target = Path(os.environ.get('CARGO_TARGET_DIR', root/'target'))
    if not target.is_absolute(): target = root/target
    target = target.resolve()
    profile = target/(args.target or '')/'release'
    extension = '.exe' if os.name == 'nt' else ''
    binaries = dict(reference=reference, rust=profile/('deadlua'+extension), cache=profile/('examples/os_process'+extension))
    artifacts = {name: dict(path=str(path), sha256=sha(path)) for name, path in binaries.items()}
    results = []
    with tempfile.TemporaryDirectory(prefix='deadlua-upvalue-names-', dir=args.run_root) as temporary:
        work = Path(temporary)
        (work/'source.lua').write_bytes(fixture.read_bytes())
        env = os.environ.copy()
        for name in ['LUA_INIT', 'LUA_PATH', 'LUA_CPATH']: env.pop(name, None)
        compile_command = [str(reference), '-e', "local f=assert(loadfile('source.lua'));local out=assert(io.open('original.luac','wb'));out:write(string.dump(f));assert(out:close())"]
        compiled = subprocess.run(compile_command, cwd=work, env=env, capture_output=True, timeout=10)
        assert compiled.returncode == 0 and not compiled.stdout and not compiled.stderr
        original = (work/'original.luac').read_bytes()
        assert rewrite(original)[0] == original
        for names in range(3):
            for style in ['original', 'empty', 'nul']:
                for strip in [False, True]:
                    data, targets = rewrite(original, names, style, strip)
                    name = f'names{names}-{style}-locations{int(not strip)}'
                    (work/'upvalue.luac').write_bytes(data)
                    cache = work/name
                    runs = {}
                    for engine in ['reference', 'rust', 'rust_cache_store', 'rust_cache_hit']:
                        settings = env.copy()
                        if engine.startswith('rust_cache_'):
                            settings['DEADLIB_LUA_TEST_CACHE'] = str(cache)
                            invocation = [str(binaries['cache']), 'rust-cache', 'upvalue.luac']
                        else:
                            invocation = [str(binaries[engine]), 'upvalue.luac']
                        try:
                            run = subprocess.run(invocation, cwd=work, env=settings, capture_output=True, timeout=10)
                            runs[engine] = dict(command=invocation, returncode=run.returncode, stdout_hex=run.stdout.hex(), stderr_hex=run.stderr.hex(), completion_marker=b'upvalue-names-complete' in run.stdout)
                        except subprocess.TimeoutExpired:
                            runs[engine] = dict(command=invocation, timeout=True)
                    expected = runs['reference']
                    equal = {}
                    for engine, run in runs.items():
                        stderr = b'CACHE\tStored\n' if engine.endswith('_store') else b'CACHE\tHit\n' if engine.endswith('_hit') else b''
                        actual_stderr = bytes.fromhex(run.get('stderr_hex', '')).replace(b'\r\n', b'\n')
                        equal[engine] = expected.get('returncode') == run.get('returncode') == 0 and run.get('completion_marker', False) and run.get('stdout_hex') == expected.get('stdout_hex') and actual_stderr == stderr
                    results.append(dict(name=name, name_count=names, name_style=style, locations_stripped=strip, two_capture_prototypes=targets, chunk_hex=data.hex(), chunk_sha256=hashlib.sha256(data).hexdigest(), runs=runs, equal=equal))
                    print(name, equal, flush=True)
    assert hashes() == before, 'sources changed during upvalue audit'
    for name, item in artifacts.items(): assert sha(item['path']) == item['sha256'], name
    report = dict(source_hashes=before, fixture_sha256=sha(fixture), reference=metadata, rust_build_command=command, target=args.target, reference_fp_model=args.reference_fp_model, artifacts=artifacts, compile_command=compile_command, original_chunk_hex=original.hex(), original_chunk_sha256=hashlib.sha256(original).hexdigest(), timeout_seconds=10, cases=results, passed={engine: sum(row['equal'][engine] for row in results) for engine in ['reference', 'rust', 'rust_cache_store', 'rust_cache_hit']}, bootstrap=None, limitations=__doc__)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2)+'\n')
    print(report['passed'], flush=True)
    if not all(all(row['equal'].values()) for row in results): raise SystemExit(1)


if __name__ == '__main__': main()
