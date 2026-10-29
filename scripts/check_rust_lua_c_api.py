"""Compare a pure C host and the unchanged C CLI against Lua 5.1.5 DLLs."""
from pathlib import Path
import argparse
import hashlib
import io
import json
import os
import re
import shutil
import struct
import subprocess
import sys
import tarfile
import urllib.request
from build_lua_cli_reference import URL, SHA256
from check_rust_lua_full_suite import unpack, run as run_suite
from check_rust_lua_suite import URL as SUITE_URL, SHA256 as SUITE_SHA256


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def pe_exports(path):
    data = path.read_bytes()
    pe = struct.unpack_from('<I', data, 0x3c)[0]
    sections, optional_size = struct.unpack_from('<H', data, pe + 6)[0], struct.unpack_from('<H', data, pe + 20)[0]
    optional = pe + 24
    plus = struct.unpack_from('<H', data, optional)[0] == 0x20b
    image_base = struct.unpack_from('<Q' if plus else '<I', data, optional + (24 if plus else 28))[0]
    export_rva = struct.unpack_from('<I', data, optional + (112 if plus else 96))[0]
    table = []
    for index in range(sections):
        at = optional + optional_size + index * 40
        virtual_size, rva, size, offset = struct.unpack_from('<IIII', data, at + 8)
        table.append((rva, max(size, virtual_size), offset))
    def file_offset(rva):
        start, _, offset = next(row for row in table if row[0] <= rva < row[0] + row[1])
        return offset + rva - start
    directory = file_offset(export_rva)
    count, functions, names, ordinals = struct.unpack_from('<IIII', data, directory + 24)
    result = {}
    for index in range(count):
        at = file_offset(struct.unpack_from('<I', data, file_offset(names) + 4 * index)[0])
        name = data[at:data.index(0, at)].decode()
        ordinal = struct.unpack_from('<H', data, file_offset(ordinals) + 2 * index)[0]
        rva = struct.unpack_from('<I', data, file_offset(functions) + 4 * ordinal)[0]
        result[name] = dict(rva=rva, address=image_base + rva, bytes=data[file_offset(rva):file_offset(rva) + 16].hex())
    return result, image_base


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--package', type=Path, required=True)
    parser.add_argument('--work', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--target', choices=['i686-unknown-linux-gnu'])
    parser.add_argument('--suite', action='store_true', help='Run unchanged main.lua/all.lua with native modules (Linux)')
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    package = args.package.resolve()
    work = args.work.resolve()
    work.mkdir(parents=True, exist_ok=True)
    report = json.loads((package / 'manifest.json').read_text())
    for name, digest in report['source_hashes'].items():
        assert sha(root / name) == digest, name
    for name, digest in report['payload_hashes'].items():
        assert sha(package / name) == digest, name
    for name, target in report['symlinks'].items():
        assert (package / name).is_symlink() and os.readlink(package / name) == target
    windows = os.name == 'nt'
    if args.suite and windows:
        parser.error('the native stock-suite gate currently requires Linux')
    x86 = args.target is not None
    cached = root / 'target/lua-cli-reference/lua-5.1.5.tar.gz'
    data = cached.read_bytes() if cached.exists() else urllib.request.urlopen(URL, timeout=30).read()
    assert hashlib.sha256(data).hexdigest() == SHA256
    with tarfile.open(fileobj=io.BytesIO(data), mode='r:gz') as archive:
        assert all((work / entry.name).resolve().is_relative_to(work) for entry in archive.getmembers())
        archive.extractall(work, filter='data')
    source = work / 'lua-5.1.5/src'
    if windows:
        finder = Path(os.environ['ProgramFiles(x86)']) / 'Microsoft Visual Studio/Installer/vswhere.exe'
        setup = subprocess.check_output([str(finder), '-latest', '-products', '*', '-find', 'VC/Auxiliary/Build/vcvars64.bat'], text=True).strip().splitlines()[0]
    builds = []
    artifacts = {}
    def compile_c(folder, name, sources, library=None, shared=False, checked=False, diagnostics=False):
        output = folder / (name + ('.dll' if shared else '.exe') if windows else 'liblua5.1.so.0' if name == 'lua51' else name + '.so' if shared else name)
        header = folder / 'assertions.h'
        if checked:
            header.write_text('#include <stdio.h>\n#include <stdlib.h>\n#define lua_assert(e) ((e)?(void)0:(fprintf(stderr,"LUA_ASSERT %s:%d: %s\\n",__FILE__,__LINE__,#e),exit(86)))\n')
            artifacts[str(header)] = sha(header)
        if windows:
            headers = package / 'include' if folder.name == 'rust' else source
            command = ['cl', '/nologo', '/O2', '/MT', '/D_CRT_SECURE_NO_WARNINGS', '/DLUA_BUILD_AS_DLL', '/I' + str(headers)]
            if library is None:
                command += ['/DLUA_CORE']
            if shared:
                command += ['/LD']
            if checked:
                command += ['/FI' + str(header)]
            if diagnostics:
                command += ['/DCAPI_DIAGNOSTICS']
            command += list(map(str, sources)) + ([str(library)] if library else []) + ['/link', '/STACK:33554432', '/OUT:' + str(output)]
            if shared:
                command += ['/IMPLIB:' + str(folder / (name + '.lib'))]
            response = folder / (name + '.rsp')
            response.write_text(' '.join('"' + arg + '"' for arg in command[1:]))
            script = folder / (name + '.cmd')
            script.write_text('@call "' + setup + '" >nul\n@cl @"' + str(response) + '"\n@exit /b %errorlevel%\n')
            invocation = ['cmd', '/d', '/c', str(script)]
        else:
            headers = package / 'include' if folder.name == 'rust' else source
            command = ['cc', '-O2', '-D_DEFAULT_SOURCE', '-D_LARGEFILE64_SOURCE', '-D_POSIX_C_SOURCE=200809L', '-DLUA_USE_POSIX', '-DLUA_USE_DLOPEN', '-DLUA_USE_ISATTY', '-I' + str(headers)]
            if x86:
                command += ['-m32', '-msse2', '-mfpmath=sse']
            if shared:
                command += ['-shared', '-fPIC']
            if name == 'lua51':
                command += ['-Wl,-soname,liblua5.1.so.0']
            if checked:
                command += ['-include', str(header)]
            if diagnostics:
                command += ['-DCAPI_DIAGNOSTICS']
            command += list(map(str, sources)) + ([str(library)] if library else []) + ['-Wl,-rpath,' + str(folder), '-lm', '-ldl', '-o', str(output)]
            invocation = command
        process = subprocess.run(invocation, cwd=folder, capture_output=True)
        log = folder / (name + '.log')
        log.write_bytes(process.stdout + process.stderr)
        assert process.returncode == 0, (process.stdout + process.stderr).decode(errors='replace')[-4000:]
        builds.append(command)
        for path in [output, log]:
            artifacts[str(path)] = sha(path)
        return output
    def run(command, folder):
        environment = {key: value for key, value in os.environ.items() if key.upper() not in {'LUA_INIT', 'LUA_PATH', 'LUA_CPATH'}}
        process = subprocess.run(list(map(str, command)), cwd=folder, env=environment, capture_output=True, timeout=90)
        return dict(command=list(map(str, command)), returncode=process.returncode, stdout=process.stdout.decode(errors='replace').replace('\r\n', '\n'), stderr=process.stderr.decode(errors='replace').replace('\r\n', '\n'))
    runs, terminating, cli_runs, binaries, memory_runs, table_runs = {}, {}, {}, {}, {}, {}
    for engine in ['reference', 'checked', 'rust']:
        folder = work / engine
        folder.mkdir(exist_ok=True)
        if engine == 'rust':
            runtime = Path(report['runtime'])
            deployed = folder / ('lua51.dll' if windows else 'liblua5.1.so.0')
            shutil.copyfile(runtime, deployed)
            library = package / 'lib/lua51.lib' if windows else deployed
        else:
            runtime = compile_c(folder, 'lua51', sorted(path for path in source.glob('*.c') if path.name not in {'lua.c', 'luac.c', 'print.c'}), shared=True, checked=engine == 'checked')
            library = folder / 'lua51.lib' if windows else runtime
        artifacts[str(runtime)] = sha(runtime)
        module = compile_c(folder, 'probe', [root / 'crates/deadlib-lua/capi/tests/capi-module.c'], library, shared=True)
        host = compile_c(folder, 'host', [root / 'crates/deadlib-lua/capi/tests/capi-host.c'], library, diagnostics=engine == 'rust')
        cli_source = source / 'lua.c'
        if engine == 'rust':
            cli_source = folder / 'lua.c'
            shutil.copyfile(source / 'lua.c', cli_source)
            assert sha(cli_source) == sha(source / 'lua.c')
            artifacts[str(cli_source)] = sha(cli_source)
        cli = compile_c(folder, 'lua', [cli_source], library)
        runs[engine] = run([host, '--module', module], folder)
        terminating[engine] = {symbol: run([host, symbol], folder) for symbol in ['owned_default_panic', 'owned_returning_panic', '--memory-default-panic', '--memory-returning-panic', '--memory-child-panic', '--table-child-panic']}
        memory_runs[engine] = run([host, '--memory'], folder)
        table_runs[engine] = run([host, '--table-limits'], folder)
        program = folder / 'cli.lua'
        program.write_text("assert(_VERSION=='Lua 5.1');assert(arg[1]=='argument');local f=assert(package.loadlib(arg[2],'luaopen_capi_probe'));local m=f();assert(m.sum(17,25)==42);local co=coroutine.create(function()coroutine.yield(17);return 42 end);assert(select(2,coroutine.resume(co))==17);assert(select(2,coroutine.resume(co))==42);print('standalone-cli-ok',debug.getinfo(1,'S').what)\n")
        cli_runs[engine] = run([cli, program, 'argument', module], folder)
        artifacts[str(program)] = sha(program)
        binaries[engine] = dict(host=str(host), module=str(module), cli=str(cli), runtime=str(runtime))
        print(engine, runs[engine]['returncode'], len(runs[engine]['stdout'].splitlines()), cli_runs[engine]['returncode'], flush=True)
    expected = (root / 'crates/deadlib-lua/capi/tests/capi-host.expected').read_text().splitlines()
    assert len(expected) == 97
    for engine, row in runs.items():
        assert row['returncode'] == 0 and row['stdout'].splitlines() == expected, engine
        if engine == 'rust':
            assert re.fullmatch(r'CAPI operations=\d+ states=6 groups=5 libraries=1\n', row['stderr']), row['stderr']
        else:
            assert not row['stderr'], row['stderr']
    memory_expected = (root / 'crates/deadlib-lua/capi/tests/memory.expected').read_text().splitlines()
    for engine, row in memory_runs.items():
        assert row['returncode'] == 0 and not row['stderr'] and row['stdout'].splitlines() == memory_expected, (engine, row)
    table_expected = (root / 'crates/deadlib-lua/capi/tests/table-limits.expected').read_text().splitlines()
    for engine, row in table_runs.items():
        assert row['returncode'] == 0 and not row['stderr'] and row['stdout'].splitlines() == table_expected, (engine, row)
    for symbol in terminating['reference']:
        rows = [terminating[engine][symbol] for engine in runs]
        assert all(row['returncode'] == 1 for row in rows)
        assert len({(row['stdout'], row['stderr']) for row in rows}) == 1
    assert all(row['returncode'] == 0 and not row['stderr'] for row in cli_runs.values()), cli_runs
    assert len({row['stdout'] for row in cli_runs.values()}) == 1
    runtime = Path(report['runtime'])
    if windows:
        exports, image_base = pe_exports(runtime)
        link_map = package / 'metadata/forwarding.map'
        private = {name: int(address, 16) for name, address in re.findall(r'^\s*[0-9a-fA-F]+:[0-9a-fA-F]+\s+(deadlua_c_\w+)\s+([0-9a-fA-F]+)', link_map.read_text(), re.M)}
        artifacts[str(link_map)] = sha(link_map)
        for name in report['generated_exports']:
            entry = bytes.fromhex(exports[name]['bytes'])
            address = exports[name]['address']
            if entry.startswith(b'\xf3\x0f\x1e\xfa'):
                entry = entry[4:]
                address += 4
            assert entry[0] in (0xe9, 0xeb), name
            size = 5 if entry[0] == 0xe9 else 2
            relative = int.from_bytes(entry[1:size], 'little', signed=True)
            assert address + size + relative == private['deadlua_c_' + name], name
        forwarded = report['generated_exports']
    else:
        listing = subprocess.check_output(['nm', '-D', '--defined-only', str(runtime)], text=True)
        exports = {line.split()[-1]: line.split()[0] for line in listing.splitlines()}
        forwarded = []
        for name in report['generated_exports']:
            disassembly = subprocess.check_output(['objdump', '-d', '--disassemble=' + name, str(runtime)], text=True)
            body = disassembly.split('<' + name + '>:')[1].split('\n\n')[0]
            instructions = [line for line in body.splitlines() if re.match(r'\s*[0-9a-f]+:', line)]
            assert instructions and 'jmp' in instructions[0] and '<deadlua_c_' + name + '>' in instructions[0], (name, body)
            forwarded.append(name)
    assert not any(name.startswith('deadlua_c_') for name in exports)
    assert not any(name in exports for name in report['missing_core_exports'])
    suite_results = []
    if args.suite:
        cached = root / 'target/lua-reference-suites/lua5.1-tests.tar.gz'
        suite_data = cached.read_bytes() if cached.exists() else urllib.request.urlopen(SUITE_URL, timeout=30).read()
        assert hashlib.sha256(suite_data).hexdigest() == SUITE_SHA256
        for engine in runs:
            for entry in ['main.lua', 'all.lua']:
                directory = work / 'suite' / engine / entry.removesuffix('.lua')
                directory.mkdir(parents=True, exist_ok=True)
                unpack(suite_data, directory)
                suite = directory / 'lua5.1-tests'
                sources = {path.relative_to(suite).as_posix(): sha(path) for path in suite.rglob('*') if path.is_file() and path.suffix in {'.lua', '.c', '.h'}}
                modules = []
                for name in ['lib1', 'lib11', 'lib2', 'lib21']:
                    module = suite / 'libs' / (name + '.so')
                    headers = package / 'include' if engine == 'rust' else source
                    command = ['cc', '-shared', '-fPIC', '-O2', '-I' + str(headers), str(suite / 'libs' / (name + '.c')), '-o', str(module)]
                    if x86:
                        command[1:1] = ['-m32', '-msse2', '-mfpmath=sse']
                    process = subprocess.run(command, capture_output=True)
                    assert process.returncode == 0, process.stderr.decode(errors='replace')
                    modules.append(dict(command=command, path=str(module), sha256=sha(module)))
                shutil.copyfile(suite / 'libs/lib2.so', suite / 'libs/-lib2.so')
                row = run_suite(Path(binaries[engine]['cli']), entry, suite, 180)
                if entry == 'all.lua':
                    assert b'cannot load dynamic library' not in bytes.fromhex(row['stdout_hex'])
                assert sources == {name: sha(suite / name) for name in sources}
                suite_results.append(dict(engine=engine, entry=entry, source_hashes=sources, native_modules=modules, **row))
                print('stock C CLI', engine, entry, row['status'], flush=True)
    bound_sources = dict(report['source_hashes'])
    bound_sources[Path(__file__).resolve().relative_to(root).as_posix()] = sha(Path(__file__))
    golden = root / 'crates/deadlib-lua/capi/tests/capi-host.expected'
    artifacts[str(root / 'crates/deadlib-lua/capi/tests/memory.expected')] = sha(root / 'crates/deadlib-lua/capi/tests/memory.expected')
    bound_sources[golden.relative_to(root).as_posix()] = sha(golden)
    table_golden = root / 'crates/deadlib-lua/capi/tests/table-limits.expected'
    bound_sources[table_golden.relative_to(root).as_posix()] = sha(table_golden)
    for helper in ['build_lua_cli_reference.py', 'check_rust_lua_full_suite.py', 'check_rust_lua_suite.py']:
        path = root / 'scripts' / helper
        bound_sources[path.relative_to(root).as_posix()] = sha(path)
    for name, digest in report['source_hashes'].items():
        assert sha(root / name) == digest, name
    result = dict(package_manifest=str(package / 'manifest.json'), package_manifest_sha256=sha(package / 'manifest.json'), source_hashes=bound_sources, reference_archive_sha256=SHA256, reference_source_hashes={path.name: sha(path) for path in source.iterdir() if path.suffix in {'.c', '.h'}}, builds=builds, artifacts=artifacts, runs=runs, memory_runs=memory_runs, memory_observations=len(memory_expected), table_runs=table_runs, table_observations=len(table_expected), terminating=terminating, cli_runs=cli_runs, binaries=binaries, forwarded_entries=forwarded, exports=exports, observations_equal=True, observations=97, no_rust_bootstrap=True, no_public_forwarding_frames=True, suite_results=suite_results, suite_archive_sha256=SUITE_SHA256 if args.suite else None, internal_test_module=False, full_lua51_parity=False)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + '\n')
    assert all(row['status'] == 'pass' for row in suite_results), 'stock C CLI suite failure; see report'
    print(f'97 C observations, {len(memory_expected)} allocation and {len(table_expected)} table-limit observations, panic termination, native modules and the unchanged C CLI match.', flush=True)


if __name__ == '__main__':
    main()
