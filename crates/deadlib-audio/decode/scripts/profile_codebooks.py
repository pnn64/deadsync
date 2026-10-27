"""Count real-packet codebook activity in a PCM-bit-checked instrumented copy.

The production crates are unchanged. Counts describe actual symbols/branches;
this is not a timing or allocation benchmark. Python 3.11+, standard library.
"""
from __future__ import annotations
import argparse
import csv
from datetime import datetime, timezone
import json
from pathlib import Path
import shutil
import subprocess
import tomllib
import uuid
from benchmark import DECODE, REPO, label, sha

COUNTERS = ('calls', 'singleton', 'prefix_leaf', 'prefix_continuation',
            'short_lookahead', 'no_table', 'tree_edges', 'missing_prefix')

PROFILE = '''
#[derive(Clone)]
pub struct BookProfile {
    pub scalar_calls: u64,
    pub vector_calls: u64,
    pub errors: [u64; 2],
    pub lengths: [u64; 33],
    pub entries: Vec<u64>,
}
impl BookProfile {
    fn new(entries: usize) -> Self {
        Self { scalar_calls: 0, vector_calls: 0, errors: [0; 2],
            lengths: [0; 33], entries: vec![0; entries] }
    }
}
'''


def instrument(source: str) -> str:
    if 'pub struct BookProfile' in source:
        raise ValueError('source is already instrumented')

    def replace(old, new):
        nonlocal source
        if source.count(old) != 1:
            raise ValueError(f'codebook anchor changed: {old[:100]!r}')
        source = source.replace(old, new)

    replace('const LEAF:', 'use std::cell::{Cell, RefCell};\n' + PROFILE + '\nconst LEAF:')
    replace('pub struct Huffman {', 'pub struct Huffman {\n    profile: Cell<[u64; 8]>,')
    replace('                nodes: Vec::new(),', '                profile: Cell::new([0; 8]),\n                nodes: Vec::new(),')
    replace('            nodes,\n            singleton:', '            profile: Cell::new([0; 8]),\n            nodes,\n            singleton:')
    replace('    #[inline]\n    pub fn decode(&self,', '''    fn count(&self, counter: usize) {
        let mut counts = self.profile.get(); counts[counter] += 1; self.profile.set(counts);
    }
    pub fn profile_counts(&self) -> [u64; 8] { self.profile.get() }
    pub fn profile_width(&self) -> u8 { self.prefix_bits }
    #[inline]
    pub fn decode(&self,''')
    replace('        if self.singleton != MISSING {\n            bits.read(1)?;',
            '        self.count(0);\n        if self.singleton != MISSING {\n            self.count(1);\n            bits.read(1)?;')
    replace('        let width = self.prefix_bits as u32;\n        if width != 0', '''        let width = self.prefix_bits as u32;
        if width == 0 { self.count(5); }
        else if bits.is_exhausted() || bits.remaining() < width as usize { self.count(4); }
        if width != 0''')
    replace('                if consumed != 0 {\n                    bits.consume(consumed);',
            '                if consumed != 0 {\n                    self.count(2);\n                    bits.consume(consumed);')
    replace('                bits.consume(width);', '                self.count(3);\n                bits.consume(width);')
    replace('                node = (entry >> 4) as usize;\n            }',
            '                node = (entry >> 4) as usize;\n            } else { self.count(7); }')
    replace('            let next = self.nodes[node][bits.read(1)? as usize];',
            '            let next = self.nodes[node][bits.read(1)? as usize];\n            self.count(6);')
    replace('pub struct Codebook {', 'pub struct Codebook {\n    profile: RefCell<BookProfile>,')
    replace('        Ok(Self {\n            dimensions,',
            '        Ok(Self {\n            profile: RefCell::new(BookProfile::new(lengths.len())),\n            dimensions,')
    replace('    #[inline]\n    pub fn decode_scalar(&self, bits: &mut BitReader<\'_>) -> Result<u32, Error> {\n        self.huffman.decode(bits)\n    }', '''    pub fn profile(&self) -> BookProfile { self.profile.borrow().clone() }
    #[inline]
    pub fn decode_scalar(&self, bits: &mut BitReader<'_>) -> Result<u32, Error> {
        let start = bits.position();
        let result = self.huffman.decode(bits);
        let mut profile = self.profile.borrow_mut();
        profile.scalar_calls += 1;
        match result {
            Ok(entry) => {
                let length = bits.position() - start;
                assert_eq!(length, self.lengths[entry as usize] as usize);
                profile.lengths[length] += 1;
                profile.entries[entry as usize] += 1;
            }
            Err(Error::EndOfPacket) => profile.errors[0] += 1,
            Err(_) => profile.errors[1] += 1,
        }
        result
    }''')
    replace('    pub fn decode_vector(&self, bits: &mut BitReader<\'_>) -> Result<&[f32], Error> {',
            '    pub fn decode_vector(&self, bits: &mut BitReader<\'_>) -> Result<&[f32], Error> {\n        self.profile.borrow_mut().vector_calls += 1;')
    return source


def table(path, rows):
    with path.open('w', encoding='utf8', newline='') as output:
        writer = csv.DictWriter(output, list(rows[0]), delimiter='\t', lineterminator='\n')
        writer.writeheader(); writer.writerows(rows)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('files', type=Path, nargs='*')
    parser.add_argument('--run-root', type=Path)
    parser.add_argument('--allow-zero-span-granules', action='store_true')
    args = parser.parse_args()
    run = (args.run_root or REPO / 'target/deadlib-audio-reference' / ('codebooks-' + uuid.uuid4().hex)).resolve()
    run.mkdir(parents=True, exist_ok=False)
    files = args.files or [DECODE / 'vorbis/tests/fixtures' / name for name in ('mono.ogg', 'stereo.ogg', 'surround.ogg')]
    if not args.files:
        files += [REPO / 'assets/music/in_two (loop).ogg']
        for name in ('itgmania/goin-under.ogg', 'ffmpeg/coyote.ogg', 'ffmpeg/test6.ogg'):
            path = REPO / 'target/deadlib-audio-reference/corpus' / name
            if path.exists(): files.append(path)
    files = [p.resolve() for p in files]
    driver = DECODE / 'vorbis/benches/support/codebook_profile.rs'
    source_files = [p for package in ('ogg', 'vorbis') for p in (DECODE / package / 'src').glob('*.rs')]
    source_files += [Path(__file__).resolve(), driver, DECODE / 'scripts/benchmark.py',
        DECODE / 'ogg/Cargo.toml', DECODE / 'vorbis/Cargo.toml', REPO / 'Cargo.toml', REPO / '.cargo/config.toml']
    sources = {label(p): sha(p) for p in sorted(source_files)}
    generated = run / 'profiled/src'; generated.mkdir(parents=True)
    for p in (DECODE / 'vorbis/src').glob('*.rs'):
        text = p.read_text(encoding='utf8')
        if p.name == 'codebook.rs': text = instrument(text)
        (generated / p.name).write_bytes(text.encode())
    dependency = json.dumps((DECODE / 'ogg').as_posix())
    (run / 'profiled/Cargo.toml').write_bytes(('''[package]
name = "deadlib-vorbis-codebook-profile"
version = "0.0.0"
edition = "2024"
publish = false
[dependencies]
deadlib-audio-decode-ogg = { path = ''' + dependency + ' }\n').encode())
    (run / 'src').mkdir(); shutil.copyfile(driver, run / 'src/main.rs')
    cargo = '''[package]
name = "deadlib-vorbis-codebook-probe"
version = "0.0.0"
edition = "2024"
publish = false
[workspace]
[dependencies]
profiled = { package = "deadlib-vorbis-codebook-profile", path = "profiled" }
'''
    for package in ('ogg', 'vorbis'):
        cargo += f'deadlib-audio-decode-{package} = {{ path = {json.dumps((DECODE / package).as_posix())} }}\n'
    profiles = tomllib.loads((REPO / 'Cargo.toml').read_text(encoding='utf8'))['profile']
    profile = profiles['release'] | profiles['bench']
    cargo += '\n[profile.release]\n' + ''.join(f'{key} = {json.dumps(value)}\n' for key, value in profile.items())
    (run / 'Cargo.toml').write_bytes(cargo.encode())
    build = subprocess.run(['cargo', 'build', '--offline', '--release', '--manifest-path', str(run / 'Cargo.toml'), '--message-format=json'], cwd=REPO, capture_output=True)
    (run / 'build.log').write_bytes(build.stdout + build.stderr)
    messages = [json.loads(line) for line in build.stdout.decode().splitlines() if line.startswith('{')]
    if build.returncode:
        raise RuntimeError(''.join(m['message'].get('rendered', '') for m in messages if m.get('reason') == 'compiler-message') + build.stderr.decode())
    artifact = next(m for m in messages if m.get('executable') and m['target']['name'] == 'deadlib-vorbis-codebook-probe')
    binary = Path(artifact['executable'])
    manifest = {'started_utc': datetime.now(timezone.utc).isoformat(), 'git_head': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=REPO, text=True).strip(),
        'rustc': subprocess.check_output(['rustc', '-Vv'], cwd=REPO, text=True), 'source_sha256': sources,
        'input_sha256': {label(p): sha(p) for p in files}, 'binary_path': label(binary), 'binary_sha256': sha(binary),
        'generated_sha256': {p.relative_to(run).as_posix(): sha(p) for p in run.rglob('*') if p.is_file() and p.suffix in ('.rs', '.toml') and 'target' not in p.relative_to(run).parts},
        'profile_release': profile, 'cargo_artifact_profile': artifact['profile'],
        'allow_zero_span_granules': args.allow_zero_span_granules, 'huffman_counter_order': COUNTERS,
        'scope': 'Counts only from a PCM-bit-checked instrumented full-crate copy. RefCell/Cell counters and frequency allocations perturb code layout, cost and memory; no timing or memory comparison. Setup role masks describe possible references, not per-call contexts. Hypothetical prefix coverage ignores packet-tail lookahead and table-size tradeoffs.', 'files': []}
    books = []; symbols = []; summaries = []
    for number, path in enumerate(files):
        result = subprocess.run([str(binary), str(path), str(int(args.allow_zero_span_granules))], cwd=REPO, capture_output=True)
        (run / f'worker-{number}.log').write_bytes(result.stdout + result.stderr)
        if result.returncode: raise RuntimeError(result.stderr.decode())
        records = [json.loads(line) for line in result.stdout.decode().splitlines()]
        metadata = records[0]; assert metadata['bit_exact'] and metadata['raw_channel_samples'] > 0
        assert len(records) == metadata['books'] + 1
        manifest['files'].append({'path': label(path), **metadata})
        file_books = []
        for book in records[1:]:
            hist = book.pop('length_counts'); counts = book.pop('entry_counts'); huffman = book.pop('huffman')
            assert len(hist) == 33 and len(huffman) == len(COUNTERS)
            assert sum(counts) == sum(hist) == book['scalar_calls'] - sum(book['errors'])
            assert sum(huffman[i] for i in (1, 2, 3, 4, 5)) == book['scalar_calls'] == huffman[0]
            assert book['vector_calls'] <= book['scalar_calls'] and huffman[7] == 0
            errors = book.pop('errors'); lengths = book.pop('codeword_lengths')
            assert len(lengths) == len(counts) == book['entries']
            successes = sum(hist)
            row = {'path': label(path), **book, 'max_codeword_bits': max(lengths), 'successes': successes, 'end_of_packet_errors': errors[0], 'other_errors': errors[1],
                'mean_codeword_bits': sum(i * n for i, n in enumerate(hist)) / max(1, successes)}
            row.update(dict(zip(COUNTERS[1:], huffman[1:])))
            for width in (8, 9, 10, 12): row[f'length_at_most_{width}_fraction'] = sum(hist[:width + 1]) / max(1, successes)
            for extra in (0, 1, 2):
                width = min(book['prefix_bits'] + extra, 8, max(lengths)) if book['prefix_bits'] else 0
                row[f'modeled_prefix_leaves_extra_{extra}'] = sum(hist[:width + 1]) if width else 0
                row[f'logical_prefix_bytes_extra_{extra}'] = 4 << width if width else 0
            books.append(row); file_books.append(row)
            for entry, count in enumerate(counts):
                if count: symbols.append({'path': label(path), 'book': book['book'], 'entry': entry, 'codeword_bits': lengths[entry], 'calls': count})
        summary = {'path': label(path), 'raw_channel_samples': metadata['raw_channel_samples']}
        for field in ('scalar_calls', 'vector_calls', 'successes', 'end_of_packet_errors', 'other_errors', *COUNTERS[1:]):
            summary[field] = sum(row[field] for row in file_books)
        for field in ('vector_calls', 'prefix_leaf', 'prefix_continuation', 'short_lookahead', 'no_table', 'singleton'):
            summary[field + '_fraction'] = summary[field] / max(1, summary['scalar_calls'])
        summary['tree_edges_per_call'] = summary['tree_edges'] / max(1, summary['scalar_calls'])
        for extra in (0, 1, 2):
            for field in ('modeled_prefix_leaves', 'logical_prefix_bytes'):
                key = f'{field}_extra_{extra}'; summary[key] = sum(row[key] for row in file_books)
        summaries.append(summary)
        print(f'{label(path)}: {metadata["raw_channel_samples"]} PCM sample bit patterns checked; codebook counts retained', flush=True)
    table(run / 'books.tsv', books); table(run / 'symbols.tsv', symbols); table(run / 'summary.tsv', summaries)
    for mapping in (sources, manifest['input_sha256']):
        for name, digest in mapping.items():
            if sha(REPO / name) != digest: raise RuntimeError(f'source/input drift: {name}')
    assert sha(binary) == manifest['binary_sha256']
    manifest.update(completed_utc=datetime.now(timezone.utc).isoformat(), book_rows=len(books), symbol_rows=len(symbols), source_input_binary_hashes_rechecked=True,
        role_bits={'floor_scalar': 1, 'floor_vector': 2, 'residue_classword': 4, 'residue_vector': 8},
        worker_log_sha256={p.name: sha(p) for p in run.glob('worker-*.log')},
        table_sha256={name: sha(run / name) for name in ('books.tsv', 'symbols.tsv', 'summary.tsv')})
    (run / 'manifest.json').write_bytes((json.dumps(manifest, indent=2) + '\n').encode())
    print(f'Retained codebook diagnostic: {run}', flush=True)


if __name__ == '__main__': main()
