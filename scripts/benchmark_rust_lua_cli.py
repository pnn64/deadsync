"""Measure complete standalone Lua processes, including console compilation and I/O."""
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import time
import tomllib

from build_lua_cli_reference import SHA256, build


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    sources = list((root / 'crates/deadlib-lua').rglob('*.rs'))
    sources += [root / name for name in (
        'Cargo.toml', 'Cargo.lock', '.cargo/config.toml',
        'crates/deadlib-lua/Cargo.toml', 'scripts/benchmark_rust_lua_cli.py',
        'scripts/build_lua_cli_reference.py',
    )]
    def hashes():
        return {p.relative_to(root).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
                for p in sorted(sources)}
    before = hashes()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    command = ['cargo', 'build', '--release', '-p', 'deadlib-lua', '--bin', 'deadlua']
    run = subprocess.run(command, cwd=root, capture_output=True)
    args.output.with_suffix('.build.log').write_bytes(run.stdout + run.stderr)
    if run.returncode:
        raise SystemExit('release CLI build failed; see build log')
    cache = root / 'target/lua-cli-reference'
    reference = cache / ('lua.exe' if os.name == 'nt' else 'lua')
    if not (cache / 'build.json').exists():
        reference, metadata = build(root)
    else:
        metadata = json.loads((cache / 'build.json').read_text())
    assert metadata['archive_sha256'] == SHA256
    assert hashlib.sha256((cache / 'lua-5.1.5.tar.gz').read_bytes()).hexdigest() == SHA256
    assert hashlib.sha256(reference.read_bytes()).hexdigest() == metadata['binary_sha256']
    assert all(hashlib.sha256((cache / 'lua-5.1.5/src' / p).read_bytes()).hexdigest() == h
               for p, h in metadata['source_hashes'].items())
    rust = root / ('target/release/deadlua.exe' if os.name == 'nt' else 'target/release/deadlua')
    version = tomllib.loads((root / 'crates/deadlib-lua/Cargo.toml').read_text())['package']['version']
    newline = b'\r\n' if os.name == 'nt' else b'\n'
    banners = {'reference': b'Lua 5.1.5  Copyright (C) 1994-2012 Lua.org, PUC-Rio' + newline,
               'rust': f'DeadLua {version} (Lua 5.1)'.encode() + newline}
    environment = os.environ.copy()
    for name in ('LUA_INIT', 'LUA_PATH', 'LUA_CPATH'):
        environment.pop(name, None)
    fixtures = [
        ('empty_command', ['-e', ''], b'', 0, 10),
        ('stdin_source', [], b'local a=6;assert(a*7==42)', 0, 10),
        ('console_1', ['-i'], b'=6*7\n', 1, 10),
        ('console_100', ['-i'], b'=6*7\n' * 100, 100, 5),
        ('console_1000', ['-i'], b'=6*7\n' * 1000, 1000, 3),
        ('console_errors_100', ['-i'], b"error('bad',0)\n" * 100, 100, 5),
        ('debug_console_1', ['-e', 'debug.debug();assert(n==42)'], b'n=42\ncont\n', 1, 10),
        ('debug_console_100', ['-e', 'n=0;debug.debug();assert(n==4200)'], b'n=n+42\n' * 100 + b'cont\n', 100, 5),
        ('debug_console_1000', ['-e', 'n=0;debug.debug();assert(n==42000)'], b'n=n+42\n' * 1000 + b'cont\n', 1000, 3),
        ('debug_console_errors_100', ['-e', 'debug.debug()'], b"error('bad',0)\n" * 100 + b'cont\n', 100, 5),
    ]
    samples, observations = [], {}
    for name, arguments, stdin, chunks, repeats in fixtures:
        def invoke(engine):
            start = time.perf_counter_ns()
            process = subprocess.run(['lua', *arguments], executable=rust if engine == 'rust' else reference,
                                     cwd=root, env=environment, input=stdin, capture_output=True, timeout=30)
            duration = time.perf_counter_ns() - start
            observed = dict(returncode=process.returncode, stdout_hex=process.stdout.hex(), stderr_hex=process.stderr.hex())
            stderr = process.stderr
            if stderr.startswith(banners[engine]):
                stderr = stderr[len(banners[engine]):]
            behavior = dict(returncode=process.returncode, stdout_hex=process.stdout.hex(), stderr_hex=stderr.hex())
            return duration, observed, behavior
        _, c_raw, expected = invoke('reference')
        _, rust_raw, behavior = invoke('rust')
        assert expected == behavior and expected['returncode'] == 0, name
        observations[name] = dict(arguments=arguments, stdin_hex=stdin.hex(), chunks=chunks,
                                  reference=c_raw, rust=rust_raw, equal_without_version_banner=True)
        for sample in range(5):
            order = ('rust', 'reference') if sample % 2 == 0 else ('reference', 'rust')
            for engine in order:
                total = 0
                for _ in range(repeats):
                    duration, _, behavior = invoke(engine)
                    assert expected == behavior, (name, engine, sample)
                    total += duration
                samples.append(dict(workload=name, sample=sample, engine=engine, repeats=repeats,
                                    ns_per_process=total / repeats,
                                    ns_per_console_chunk=total / repeats / chunks if chunks else None))
        print(name, 'measured', flush=True)
    assert hashes() == before, 'sources changed during measurement'
    ratios = {}
    for name in observations:
        paired = {engine: {s['sample']: s['ns_per_process'] for s in samples
                           if s['engine'] == engine and s['workload'] == name}
                  for engine in ('rust', 'reference')}
        ratios[name] = statistics.median(paired['rust'][i] / paired['reference'][i] for i in range(5))
    result = dict(source_hashes=before, reference=metadata,
                  rust_binary_sha256=hashlib.sha256(rust.read_bytes()).hexdigest(),
                  version_banners={k: v.hex() for k, v in banners.items()},
                  platform=platform.platform(), command=command, samples=samples,
                  observations=observations, median_paired_rust_over_lua51=ratios,
                  limitations='Shared host; complete process wall time includes spawn, startup, compilation, execution, pipes and shutdown. Validation occurs outside timing. Console output differs only by the recorded leading version banner. No allocation/RSS/cycle counts or terminal/signal measurements.')
    args.output.write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(ratios, indent=2))


if __name__ == '__main__':
    main()
