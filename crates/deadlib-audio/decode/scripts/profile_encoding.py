"""Profile actual fixed-book encoder packets after independent round-trip checks.

This parser intentionally recognizes the initial flat-floor/ordered-eight-bit
configuration only. It rejects other setups rather than guessing their grammar.
"""
import argparse
import csv
import json
import math
from pathlib import Path
from benchmark import sha
from benchmark_encoding import write_json, write_tsv


class Bits:
    def __init__(self, data):
        self.data, self.position = data, 0

    def read(self, count):
        if self.position + count > len(self.data) * 8:
            raise ValueError('truncated encoded packet')
        byte, shift = divmod(self.position, 8)
        value = int.from_bytes(self.data[byte:byte + (shift + count + 7) // 8], 'little')
        self.position += count
        return (value >> shift) & ((1 << count) - 1)

    def expect(self, count, value):
        if self.read(count) != value:
            raise ValueError('unsupported encoder configuration or malformed packet')

    def finish(self):
        remaining = len(self.data) * 8 - self.position
        if remaining >= 8 or self.read(remaining):
            raise ValueError('unconsumed packet data or nonzero padding')


def packets(path):
    data = path.read_bytes()
    offset, partial, output = 0, bytearray(), []
    while offset < len(data):
        if data[offset:offset + 5] != b'OggS\0':
            raise ValueError('Ogg framing')
        laces = data[offset + 27:offset + 27 + data[offset + 26]]
        body = offset + 27 + len(laces)
        for size in laces:
            partial.extend(data[body:body + size])
            body += size
            if size < 255:
                output.append(bytes(partial))
                partial.clear()
        offset = body
    if offset != len(data) or partial:
        raise ValueError('truncated Ogg packets')
    return output


def configuration(packets, stages):
    id = packets[0]
    if len(id) != 30 or id[:7] != b'\x01vorbis' or packets[2][:7] != b'\x05vorbis':
        raise ValueError('Vorbis headers')
    channels = id[11]
    if (id[28] & 15) != id[28] >> 4 or not channels:
        raise ValueError('requires equal legal block sizes')
    block = 1 << (id[28] & 15)
    if block < 64 or block > 8192:
        raise ValueError('block size')
    bits = Bits(packets[2][7:])
    bits.expect(8, stages)
    for book in range(stages + 1):
        bits.expect(24, 0x564342)
        bits.expect(16, 1 if book == 0 else 2)
        bits.expect(24, 2 if book == 0 else 256)
        bits.expect(1, 1)
        bits.expect(5, 0 if book == 0 else 7)
        bits.expect(2 if book == 0 else 9, 2 if book == 0 else 256)
        bits.expect(4, 0 if book == 0 else 1)
        if book:
            bits.read(32)
            bits.read(32)
            bits.expect(4, 3)
            bits.expect(1, 0)
            for value in range(16):
                bits.expect(4, value)
    for count, value in [(6, 0), (16, 0), (6, 0), (16, 1), (5, 0), (2, 0),
                         (4, block.bit_length() - 2), (6, 0), (16, 1), (24, 0),
                         (24, block // 2), (24, 15), (6, 1), (8, 0), (3, 0), (1, 0)]:
        bits.expect(count, value)
    cascade = (1 << stages) - 1
    bits.expect(3, cascade & 7)
    bits.expect(1, int(cascade > 7))
    if cascade > 7:
        bits.expect(5, cascade >> 3)
    for book in range(1, stages + 1):
        bits.expect(8, book)
    for count, value in [(6, 0), (16, 0), (1, 0), (1, 0), (2, 0), (8, 0), (8, 0),
                         (8, 0), (6, 0), (1, 0), (16, 0), (16, 0), (8, 0), (1, 1)]:
        bits.expect(count, value)
    bits.finish()
    return channels, block


def profile(path, stages):
    stream = packets(path)
    channels, block = configuration(stream, stages)
    partitions = block // 32
    counts = [0] * 256
    reversal = [int(f'{i:08b}'[::-1], 2) for i in range(256)]
    for packet in stream[3:]:
        bits = Bits(packet)
        bits.expect(1, 0)
        active = []
        for _ in range(channels):
            nonzero = bits.read(1)
            active.append(nonzero)
            if nonzero:
                y = bits.read(8)
                bits.expect(8, y)
        classes = [[0] * partitions for _ in range(channels)]
        for stage in range(stages):
            for partition in range(partitions):
                if stage == 0:
                    for channel in range(channels):
                        if active[channel]:
                            classes[channel][partition] = bits.read(1)
                for channel in range(channels):
                    if active[channel] and classes[channel][partition]:
                        for _ in range(8):
                            symbol = reversal[bits.read(8)]
                            if stage == 0:
                                counts[symbol] += 1
        bits.finish()
    total = sum(counts)
    common = {119, 120, 135, 136}  # Each scalar's leading digit is seven or eight.
    corners = {0, 15, 240, 255}
    # Complete prefix-length multiset: 4/2^3 + 4/2^8 + 248/2^9 = 1.
    proposed_bits = sum(n * (3 if i in common else 8 if i in corners else 9) for i, n in enumerate(counts))
    entropy = -sum((n / total) * math.log2(n / total) for n in counts if n) if total else 0
    return counts, {'channels': channels, 'block_size': block, 'stages': stages, 'first_stage_symbols': total,
                    'central_symbols': sum(counts[i] for i in common),
                    'central_fraction': sum(counts[i] for i in common) / total if total else 0,
                    'entropy_bits_per_symbol': entropy,
                    'current_first_stage_bits': total * 8, 'proposed_first_stage_bits': proposed_bits,
                    'projected_payload_saving_bytes': (total * 8 - proposed_bits) / 8}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('run', type=Path)
    args = parser.parse_args()
    run = args.run.resolve()
    manifest = json.loads((run / 'manifest.json').read_text())
    if not manifest.get('final_hashes_verified'):
        raise ValueError('requires a completed independently verified benchmark')
    with (run / 'quality.tsv').open() as f:
        quality = list(csv.DictReader(f, delimiter='\t'))
    rows, histograms = [], {}
    inputs = list(manifest['prepared_inputs'])
    for row in quality:
        if row['engine'] != 'native':
            continue
        path = run / f"output-{inputs.index(row['path'])}-native-{row['setting']}.ogg"
        if sha(path) != row['encoded_sha256']:
            raise ValueError('encoded input hash mismatch')
        counts, stats = profile(path, int(row['setting']))
        rows.append({'path': row['path'], 'setting': row['setting'], 'encoded_sha256': sha(path), **stats})
        if row['setting'] == '2' and ('in_two' in row['path'] or 'goin-under' in row['path']):
            histograms[row['path']] = counts
    write_tsv(run / 'symbols.tsv', rows)
    write_json(run / 'symbols.json', {'analyzer_sha256': sha(Path(__file__)),
        'producer_manifest_sha256': sha(run / 'manifest.json'), 'quality_report_sha256': sha(run / 'quality.tsv'),
        'schema': 'Initial flat-floor setup with all residue VQ books ordered, fixed-length eight-bit; every audio packet consumed through zero padding',
        'proposal': 'First-stage four central pair symbols length 3, four corner pairs length 8, other 248 length 9; complete tree; unchanged VQ values and quantizer',
        'projection_limit': 'Payload bit count only; excludes about 159 additional setup bytes, byte rounding, changed Ogg lacing/pages and encode/decode CPU effects. This is not an implemented optimization.',
        'first_stage_histograms_for_two_stage_song_cases': histograms})
    print(f'Profiled {len(rows)} native outputs: {run / "symbols.tsv"}')


if __name__ == '__main__':
    main()
