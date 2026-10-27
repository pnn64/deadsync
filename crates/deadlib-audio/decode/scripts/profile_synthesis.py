"""Diagnostic timing of the production decoder's stages using a checked copy.

Generates an instrumented decoder in an ignored standalone workspace. The
production crates are unchanged. PCM bits must match before any timing is used.
"""
from __future__ import annotations
import argparse
import csv
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import statistics
import subprocess
import tomllib
import uuid
from benchmark import DECODE, REPO, label, sha

STAGES = ('header', 'clear_floor', 'residue', 'coupling', 'floor_gain', 'imdct_finite', 'window', 'overlap_copy')


def instrument(source: str) -> str:
    def replace(old, new):
        nonlocal source
        if source.count(old) != 1:
            raise RuntimeError(f'decoder anchor changed: {old[:100]!r}')
        source = source.replace(old, new)

    def begin(stage):
        return f'let profile_{stage}_begin = std::time::Instant::now();\n'

    def end(stage):
        return f'self.profile[{STAGES.index(stage)}] += profile_{stage}_begin.elapsed().as_nanos();\n'

    replace('pub struct PacketDecoder {', 'pub struct PacketDecoder {\n    profile: [u128; 8],')
    replace('        Ok(Self {\n            setup,', '        Ok(Self {\n            profile: [0; 8],\n            setup,')
    replace('    pub fn setup(&self)', '''    pub fn profile(&self) -> [u128; 8] { self.profile }
    pub fn clear_profile(&mut self) { self.profile = [0; 8]; }
    pub fn setup(&self)''')
    replace('        let Some((header, mut bits)) = packet_header', '        ' + begin('header') + '        let Some((header, mut bits)) = packet_header')
    replace('        self.spectrum.fill(0.0);', end('header') + begin('clear_floor') + '        self.spectrum.fill(0.0);')
    replace('        if truncated {\n', end('clear_floor') + '        if truncated {\n')
    replace('            for &[magnitude, angle] in &mapping.coupling {', begin('residue') + '            for &[magnitude, angle] in &mapping.coupling {')
    replace('            for &[magnitude, angle] in mapping.coupling.iter().rev() {', end('residue') + begin('coupling') + '            for &[magnitude, angle] in mapping.coupling.iter().rev() {')
    replace('            for ch in 0..channels {\n                let spectrum', end('coupling') + '            for ch in 0..channels {\n' + begin('floor_gain') + '                let spectrum')
    replace('                let transform = if n == small {', end('floor_gain') + begin('imdct_finite') + '                let transform = if n == small {')
    replace('                apply_window(output, header, small, &self.short_slope, &self.long_slope);', end('imdct_finite') + begin('window') + '                apply_window(output, header, small, &self.short_slope, &self.long_slope);\n' + end('window'))
    replace('        let frames = if self.previous_size == 0 {', begin('overlap_copy') + '        let frames = if self.previous_size == 0 {')
    replace('        Ok(Some(PcmBlock {', end('overlap_copy') + '        Ok(Some(PcmBlock {')
    return source


def table(path, rows):
    with path.open('w', newline='', encoding='utf-8') as f:
        writer = csv.DictWriter(f, fieldnames=list(rows[0]), delimiter='\t', lineterminator='\n')
        writer.writeheader(); writer.writerows(rows)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('files', type=Path, nargs='*')
    parser.add_argument('--run-root', type=Path)
    parser.add_argument('--repeats', type=int, default=7)
    parser.add_argument('--min-seconds', type=float, default=0.15)
    parser.add_argument('--seed', type=int, default=1337)
    parser.add_argument('--allow-zero-span-granules', action='store_true', help='explicit legacy container exception; counted in each result')
    args = parser.parse_args()
    if args.repeats < 3 or not math.isfinite(args.min_seconds) or args.min_seconds <= 0:
        parser.error('use at least three repeats and a positive duration')
    run = (args.run_root or REPO / 'target/deadlib-audio-reference' / ('synthesis-' + uuid.uuid4().hex)).resolve()
    run.mkdir(parents=True, exist_ok=False)
    files = args.files or [DECODE / 'vorbis/tests/fixtures' / n for n in ('mono.ogg', 'stereo.ogg', 'surround.ogg')]
    if not args.files:
        files += [REPO / 'assets/music/in_two (loop).ogg']
        for part in ('itgmania/goin-under.ogg',):
            p = REPO / 'target/deadlib-audio-reference/corpus' / part
            if p.exists(): files.append(p)
    files = [p.resolve() for p in files]
    source_files = [p for folder in ('ogg/src', 'vorbis/src') for p in (DECODE / folder).rglob('*.rs')]
    driver = DECODE / 'vorbis/benches/support/synthesis_profile.rs'
    source_files += [Path(__file__).resolve(), driver, DECODE / 'scripts/benchmark.py', REPO / 'Cargo.toml', REPO / '.cargo/config.toml', DECODE / 'ogg/Cargo.toml', DECODE / 'vorbis/Cargo.toml']
    sources = {label(p): sha(p) for p in sorted(source_files)}
    generated = run / 'src'; generated.mkdir()
    original = (DECODE / 'vorbis/src/decoder.rs').read_text(encoding='utf-8')
    (generated / 'profiled_decoder.rs').write_bytes(instrument(original).encode())
    setup_source = (DECODE / 'vorbis/src/setup.rs').read_text(encoding='utf-8')
    begin = setup_source.index('pub(crate) fn read_audio_header(')
    end = setup_source.index('\nfn storage<', begin)
    (generated / 'profile_setup.rs').write_bytes(('use crate::{Mode, AudioHeader, Error, bitpack::{BitReader, ilog}};\n' + setup_source[begin:end]).encode())
    modules = ''.join(f'#[path = {json.dumps((DECODE / "vorbis/src" / (n + ".rs")).as_posix(), ensure_ascii=False)}]\nmod {n};\n'
                      for n in ('floor', 'floor_table', 'residue'))
    modules += '#[allow(dead_code)]\nmod profiled_decoder;\n#[path = "profile_setup.rs"]\nmod setup;\n'
    (generated / 'main.rs').write_bytes((modules + driver.read_text(encoding='utf-8')).encode())
    profiles = tomllib.loads((REPO / 'Cargo.toml').read_text(encoding='utf-8'))['profile']
    profile = profiles['release'] | profiles['bench']
    cargo = '[package]\nname = "deadlib-vorbis-synthesis-probe"\nversion = "0.0.0"\nedition = "2024"\npublish = false\n\n[workspace]\n\n[dependencies]\n'
    for package in ('ogg', 'vorbis'):
        cargo += f'deadlib-audio-decode-{package} = {{ path = {json.dumps((DECODE / package).as_posix(), ensure_ascii=False)} }}\n'
    cargo += '\n[profile.release]\n' + ''.join(f'{k} = {json.dumps(v)}\n' for k,v in profile.items())
    (run / 'Cargo.toml').write_bytes(cargo.encode())
    build = subprocess.run(['cargo', 'build', '--offline', '--release', '--manifest-path', str(run / 'Cargo.toml'), '--message-format=json'], cwd=REPO, capture_output=True)
    (run / 'build.log').write_bytes(build.stdout + build.stderr)
    if build.returncode:
        messages = [json.loads(line) for line in build.stdout.decode().splitlines() if line.startswith('{')]
        errors = ''.join(m['message'].get('rendered', '') for m in messages if m.get('reason') == 'compiler-message')
        raise RuntimeError(errors + build.stderr.decode(errors='replace'))
    artifacts = [json.loads(line) for line in build.stdout.decode().splitlines() if line.startswith('{')]
    artifact = next(a for a in artifacts if a.get('executable') and a['target']['name'] == 'deadlib-vorbis-synthesis-probe')
    binary = Path(artifact['executable'])
    manifest = {'started_utc': datetime.now(timezone.utc).isoformat(), 'git_head': subprocess.check_output(['git','rev-parse','HEAD'],cwd=REPO,text=True).strip(),
                'rustc': subprocess.check_output(['rustc','-Vv'],cwd=REPO,text=True), 'source_sha256':sources,
                'input_sha256':{label(p):sha(p) for p in files}, 'binary_path':label(binary), 'binary_sha256':sha(binary),
                'generated_sha256':{p.relative_to(run).as_posix():sha(p) for p in generated.glob('*.rs')},
                'cargo_artifact_profile':artifact['profile'], 'profile_release':profile,
                'rustflags_environment':{k:os.environ.get(k) for k in ('RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS')},
                'repeats':args.repeats,'min_seconds':args.min_seconds,'seed':args.seed,'stages':STAGES,
                'allow_zero_span_granules':args.allow_zero_span_granules,
                'scope':'Diagnostic instrumented decoder copy; per-stage Instant timers and added counters affect code layout. Actual production raw decoder is measured separately. No container IO/setup/trim in timing. Stage shares exclude instrumentation/driver overhead; no cross-library ranking.', 'files':[]}
    rows = []
    for i,path in enumerate(files):
        result = subprocess.run([str(binary),str(path),str(args.repeats),str(args.min_seconds),str(args.seed+i),str(int(args.allow_zero_span_granules))], cwd=REPO,capture_output=True)
        (run / f'worker-{i}.log').write_bytes(result.stdout + result.stderr)
        if result.returncode: raise RuntimeError(result.stderr.decode(errors='replace'))
        messages = [json.loads(line) for line in result.stdout.decode().splitlines()]
        metadata = messages[0]; assert metadata['bit_exact'] and metadata['raw_channel_samples'] > 0
        manifest['files'].append({'path':label(path),**metadata})
        for row in messages[1:]:
            assert row['samples'] == metadata['raw_channel_samples'] * row['iterations']
            assert row['packets'] == metadata['audio_packets'] * row['iterations']
            rows.append({'path':label(path),**row})
        print(f'{label(path)}: bit-exact copy; {args.repeats} interleaved raw/profiled trials',flush=True)
    summary=[]
    for path in files:
        for engine in ('production','profiled'):
            group=[r for r in rows if r['path']==label(path) and r['engine']==engine]
            times=[r['elapsed_ns']/r['iterations'] for r in group]; median=statistics.median(times)
            entry={'path':label(path),'engine':engine,'median_ns':median,'min_ns':min(times),'max_ns':max(times),
                   'mad_ns':statistics.median(abs(x-median) for x in times)}
            for j,stage in enumerate(STAGES):
                entry[stage+'_ns']=statistics.median(r['profile_ns'][j]/r['iterations'] for r in group)
            summary.append(entry)
    for row in rows:
        for stage,n in zip(STAGES,row.pop('profile_ns')): row[stage+'_ns']=n
    table(run/'trials.tsv',rows);table(run/'summary.tsv',summary)
    for mapping in (sources,manifest['input_sha256']):
        for name,digest in mapping.items():
            if sha(REPO/name)!=digest:raise RuntimeError(f'source/input drift: {name}')
    assert sha(binary)==manifest['binary_sha256']
    manifest.update(completed_utc=datetime.now(timezone.utc).isoformat(),timing_rows=len(rows),source_input_binary_hashes_rechecked=True)
    (run/'manifest.json').write_bytes((json.dumps(manifest,indent=2)+'\n').encode())
    print(f'Retained synthesis diagnostic: {run}',flush=True)


if __name__ == '__main__': main()
