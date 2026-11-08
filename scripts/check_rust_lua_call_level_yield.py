#!/usr/bin/env python3
"""Strict audit of yielding across a protected call after lua_setlevel lowers its count.

Use the source-built artifacts from a completed native-boundary proof. Any
semantic mismatch makes this audit exit 1; it is not an expected-pass allowlist.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--boundary-proof', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    proof = json.loads(args.boundary_proof.read_text())
    fixture = root/'crates/deadlib-lua/tests/native-boundary/lowered-level-yield.lua'
    before = {name: sha(root/name) for name in proof['source_hashes']}
    assert before == proof['source_hashes'], 'boundary proof source revision changed'
    assert proof['native_call_levels_probe']['observations_equal']
    assert proof['native_call_levels_probe']['completed']
    for path, digest in proof['artifacts'].items():
        assert sha(path) == digest, path
    assert fixture.relative_to(root).as_posix() in before
    runs = {}
    for engine in ['reference', 'rust']:
        previous = proof['native_call_levels_probe']['runs'][engine]['command']
        with tempfile.TemporaryDirectory(prefix='deadlua-level-yield-') as temporary:
            work = Path(temporary)
            source = work/'lowered-level-yield.lua'
            source.write_bytes(fixture.read_bytes())
            command = previous[:-1]+[str(source)]
            env = os.environ.copy()
            for name in ['LUA_INIT', 'LUA_PATH', 'LUA_CPATH']:
                env.pop(name, None)
            directory = Path(previous[0] if engine == 'reference' else previous[1]).parent
            env['PATH'] = str(directory)+os.pathsep+env.get('PATH', '')
            try:
                run = subprocess.run(command, cwd=work, env=env, capture_output=True, timeout=10)
                runs[engine] = dict(command=command, returncode=run.returncode,
                                    stdout_hex=run.stdout.hex(), stderr_hex=run.stderr.hex())
            except subprocess.TimeoutExpired:
                runs[engine] = dict(command=command, timeout=True)
    assert {name: sha(root/name) for name in before} == before
    reference, rust = runs['reference'], runs['rust']
    match = re.fullmatch(rb'BOUNDARY operations=(\d+) drops=\1 panic_abort=true\r?\nSTATES created=(\d+) collected=(\d+) destroyed=\2\r?\n', bytes.fromhex(rust.get('stderr_hex', '')))
    guards = bool(match and int(match[1]) > 0 and int(match[3]) < int(match[2]))
    equal = reference.get('returncode') == rust.get('returncode') == 0 and not reference.get('stderr_hex') and reference.get('stdout_hex') == rust.get('stdout_hex') and guards
    result = dict(source_hashes=before, fixture_sha256=sha(fixture),
                  audit_script_sha256=sha(__file__), boundary_proof=str(args.boundary_proof.resolve()),
                  boundary_proof_sha256=sha(args.boundary_proof), artifacts=proof['artifacts'],
                  runs=runs, observations_equal=equal, dispatch_and_state_guards_balanced=guards,
                  timeout_seconds=10, strict_failure_on_mismatch=True,
                  full_lua_51_parity_proven=False)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2)+'\n')
    print('lowered-call-level yield observations equal:', bool(equal))
    return 0 if equal else 1


if __name__ == '__main__':
    raise SystemExit(main())
