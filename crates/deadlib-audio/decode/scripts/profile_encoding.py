"""Profile native encoder packets after independent round-trip checks.

Recognizes the flat-floor setup with the original eight-bit, compact pair or
compact scalar first book. Other configurations are rejected rather than guessed.
"""
import argparse
import csv
import hashlib
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

    def peek9(self):
        byte, shift = divmod(self.position, 8)
        return (int.from_bytes(self.data[byte:byte + 3], 'little') >> shift) & 511

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


def first_decoder(lengths):
    # Independent exhaustive prefix occupancy, not the production free-prefix
    # allocator. The table includes all suffix aliases for each short word.
    occupied = [False] * 512
    table = [None] * 512
    for entry, length in enumerate(lengths):
        width = 1 << (9 - length)
        for prefix in range(1 << length):
            start = prefix * width
            if not any(occupied[start:start + width]):
                occupied[start:start + width] = [True] * width
                wire = int(f'{prefix:0{length}b}'[::-1], 2)
                for suffix in range(width):
                    table[wire | (suffix << length)] = (entry, length)
                break
        else:
            raise ValueError('overspecified first-stage tree')
    if not all(occupied) or any(x is None for x in table):
        raise ValueError('underspecified first-stage tree')
    return table


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
        dimensions = bits.read(16)
        entries = bits.read(24)
        if book == 1:
            if (dimensions, entries) not in {(2,256),(1,16)}:
                raise ValueError('unsupported first-stage geometry')
            first_dimensions = dimensions
        elif (dimensions, entries) != ((1,2) if book == 0 else (2,256)):
            raise ValueError('unsupported classifier/later-stage geometry')
        ordered = bits.read(1)
        if book == 1 and not ordered:
            bits.expect(1, 0)
            lengths = [bits.read(5) + 1 for _ in range(entries)]
            expected = ([1 if i == 8 else 2 if i == 7 else 5 if i in {0,15} else 6 for i in range(16)]
                        if dimensions == 1 else [3 if i in {119,120,135,136} else 8 if i in {0,15,240,255} else 9 for i in range(256)])
            if lengths != expected:
                raise ValueError('unsupported first-stage tree')
        else:
            if not ordered:
                raise ValueError('requires ordered classifier and later stages')
            bits.expect(5, 0 if book == 0 else 7)
            bits.expect(2 if book == 0 else 9, 2 if book == 0 else 256)
            if book == 1:
                if (dimensions, entries) != (2,256):
                    raise ValueError('unsupported ordered first stage')
                lengths = [8] * 256
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
    return channels, block, lengths, first_decoder(lengths), first_dimensions


def profile(path, stages):
    stream = packets(path)
    channels, block, lengths, decoder, first_dimensions = configuration(stream, stages)
    partitions = block // 32
    counts = [0] * 256
    digest = hashlib.sha256()
    reversal = [int(f'{i:08b}'[::-1], 2) for i in range(256)]
    for number, packet in enumerate(stream[3:]):
        digest.update(number.to_bytes(8, 'little'))
        bits = Bits(packet)
        bits.expect(1, 0)
        active = []
        for _ in range(channels):
            nonzero = bits.read(1)
            digest.update(bytes([nonzero]))
            active.append(nonzero)
            if nonzero:
                y = bits.read(8)
                bits.expect(8, y)
                digest.update(bytes([y, y]))
        classes = [[0] * partitions for _ in range(channels)]
        for stage in range(stages):
            for partition in range(partitions):
                if stage == 0:
                    for channel in range(channels):
                        if active[channel]:
                            classes[channel][partition] = bits.read(1)
                            digest.update(bytes([classes[channel][partition]]))
                for channel in range(channels):
                    if active[channel] and classes[channel][partition]:
                        for _ in range(8):
                            if stage == 0:
                                symbol, length = decoder[bits.peek9()]
                                bits.read(length)
                                if first_dimensions == 1:
                                    high, length = decoder[bits.peek9()]
                                    bits.read(length)
                                    symbol |= high << 4
                                counts[symbol] += 1
                            else:
                                symbol = reversal[bits.read(8)]
                            digest.update(bytes([symbol]))
        bits.finish()
    total = sum(counts)
    common = {119, 120, 135, 136}  # Each scalar's leading digit is seven or eight.
    corners = {0, 15, 240, 255}
    # Complete prefix-length multiset: 4/2^3 + 4/2^8 + 248/2^9 = 1.
    proposed_bits = sum(n * (3 if i in common else 8 if i in corners else 9) for i, n in enumerate(counts))
    def scalar_length(digit):
        return 1 if digit == 8 else 2 if digit == 7 else 5 if digit in {0,15} else 6
    scalar_bits = sum(n * (scalar_length(i & 15) + scalar_length(i >> 4)) for i, n in enumerate(counts))
    pair_lengths = ([lengths[i & 15] + lengths[i >> 4] for i in range(256)]
                    if first_dimensions == 1 else lengths)
    entropy = -sum((n / total) * math.log2(n / total) for n in counts if n) if total else 0
    return counts, {'channels': channels, 'block_size': block, 'stages': stages, 'first_stage_symbols': total,
                    'canonical_pair_count': total, 'actual_first_stage_codewords': total * (2 if first_dimensions == 1 else 1),
                    'first_book_dimensions': first_dimensions, 'first_book_entries': len(lengths),
                    'decoded_floor_class_symbol_sha256': digest.hexdigest(),
                    'central_symbols': sum(counts[i] for i in common),
                    'central_fraction': sum(counts[i] for i in common) / total if total else 0,
                    'entropy_bits_per_symbol': entropy,
                    'current_first_stage_bits': sum(n * length for n, length in zip(counts, pair_lengths)),
                    'fixed_eight_bit_first_stage_bits': total * 8, 'proposed_first_stage_bits': proposed_bits,
                    'candidate_scalar_stage_bits': scalar_bits,
                    'candidate_scalar_additional_payload_saving_bytes': (proposed_bits - scalar_bits) / 8,
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
        if row['engine'] not in {'native', 'native-baseline'}:
            continue
        path = run / f"output-{inputs.index(row['path'])}-{row['engine']}-{row['setting']}.ogg"
        if sha(path) != row['encoded_sha256']:
            raise ValueError('encoded input hash mismatch')
        counts, stats = profile(path, int(row['setting']))
        rows.append({'path': row['path'], 'engine': row['engine'], 'setting': row['setting'], 'encoded_sha256': sha(path), **stats})
        if row['setting'] == '2' and ('in_two' in row['path'] or 'goin-under' in row['path']):
            histograms[row['path'] + '/' + row['engine']] = counts
    write_tsv(run / 'symbols.tsv', rows)
    if manifest.get('baseline'):
        for row in rows:
            other_engine = 'native-baseline' if row['engine'] == 'native' else 'native'
            other = next(r for r in rows if (r['path'], r['setting'], r['engine']) == (row['path'], row['setting'], other_engine))
            if row['decoded_floor_class_symbol_sha256'] != other['decoded_floor_class_symbol_sha256']:
                raise ValueError('baseline/current decoded packet symbols changed')
    write_json(run / 'symbols.json', {'analyzer_sha256': sha(Path(__file__)),
        'producer_manifest_sha256': sha(run / 'manifest.json'), 'quality_report_sha256': sha(run / 'quality.tsv'),
        'schema': 'Flat-floor setup with original fixed, compact pair or compact scalar first stage; dimension-one entries combine into canonical digit pairs for equality checks; every packet consumed through zero padding; independent exhaustive prefix oracle',
        'proposal': 'First-stage four central pair symbols length 3, four corner pairs length 8, other 248 length 9; complete tree; unchanged VQ values and quantizer',
        'projection_limit': 'Savings column compares symbol costs to the initial fixed eight-bit book; payload bits only, excluding setup, rounding, Ogg lacing/pages and CPU. Current compact-book bits are measured separately.',
        'scalar_layout': 'Dimension-one 16-entry book, digit 8 length 1, digit 7 length 2, digits 0/15 length 5, other 12 length 6; Kraft 1/2+1/4+2/32+12/64=1. Two scalar symbols per canonical pair. Payload cost is measured for scalar inputs and projected for pair inputs; header, rounding, mux and CPU effects excluded.',
        'first_stage_histograms_for_two_stage_song_cases': histograms})
    print(f'Profiled {len(rows)} native outputs: {run / "symbols.tsv"}')


if __name__ == '__main__':
    main()
