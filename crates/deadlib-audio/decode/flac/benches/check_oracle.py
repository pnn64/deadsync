"""Audit pipeline benchmark endpoints, first-frame work, and MD5 reinitialization."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser()
    for name in ['native', 'oracle', 'input', 'output']:
        parser.add_argument('--' + name, type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    index = args.output / 'frames.index'
    ogg = args.output / 'input.oga'
    records = []

    def run(binary, mode, source, iterations=2, extra=(), env=None, failure=None):
        command = [str(binary.resolve()), mode, str(source.resolve())]
        if binary == args.oracle:
            command += [str(index.resolve()), str(iterations)]
        else:
            command += [str(iterations)] + [str(p.resolve()) for p in extra]
        result = subprocess.run(command, capture_output=True, text=True, env=env)
        record = {'command': command, 'exit_code': result.returncode,
                  'stdout': result.stdout, 'stderr': result.stderr}
        records.append(record)
        if failure:
            assert result.returncode and failure in result.stderr, record
            return
        assert result.returncode == 0, record
        fields = result.stdout.strip().split('\t')
        assert len(fields) == 9, record
        return int(fields[3]), int(fields[4])

    full = run(args.native, 'frames', args.input, extra=(index, ogg))
    first = run(args.native, 'startup', args.input)
    for mode in ['stream', 'stream-md5', 'symphonia-new', 'symphonia-new-crc',
                 'ogg-stream', 'ogg-stream-md5', 'symphonia-new-ogg', 'symphonia-new-ogg-crc']:
        assert run(args.native, mode, args.input) == full, mode
    assert run(args.native, 'symphonia-new-startup', args.input) == first
    for mode in ['libflac', 'libflac-md5', 'libflac-new', 'libflac-new-md5',
                 'libflac-new-ogg', 'libflac-new-ogg-md5']:
        assert run(args.oracle, mode, ogg if 'ogg' in mode else args.input) == full, mode
    assert run(args.oracle, 'libflac-new-startup', args.input) == first
    env = os.environ.copy()
    env['DEADLIB_FLAC_BENCH_PROBE_MD5'] = '1'
    for mode in ['libflac-md5', 'libflac-new-md5']:
        run(args.oracle, mode, args.input, env=env, failure='libFLAC digest/finish')

    bad = bytearray(args.input.read_bytes())
    assert bad[:4] == b'fLaC' and bad[4] & 127 == 0 and bad[5:8] == b'\0\0\x22'
    bad[26] ^= 1
    corrupt = args.output / 'corrupt-md5.flac'
    corrupt.write_bytes(bad)
    assert run(args.native, 'frames', corrupt) == full
    run(args.native, 'stream-md5', corrupt, failure='MD5')
    page = bytearray(ogg.read_bytes())
    end = 27 + page[26] + sum(page[27:27+page[26]])
    original_crc = int.from_bytes(page[22:26], 'little')
    page[22:26] = bytes(4)

    def crc(data):
        value = 0
        for byte in data:
            value ^= byte << 24
            for _ in range(8):
                value = ((value << 1) ^ (0x04c11db7 if value & 0x80000000 else 0)) & 0xffffffff
        return value

    assert crc(page[:end]) == original_crc
    marker = page.index(b'fLaC', 27 + page[26])
    assert marker + 42 <= end
    page[marker + 26] ^= 1
    page[22:26] = crc(page[:end]).to_bytes(4, 'little')
    corrupt_ogg = args.output / 'corrupt-md5.oga'
    corrupt_ogg.write_bytes(page)
    run(args.native, 'ogg-stream-md5', args.input, extra=(index, corrupt_ogg), failure='MD5')
    run(args.oracle, 'libflac-new-ogg-md5', corrupt_ogg, failure='libFLAC digest/finish')
    for record in records:
        for field in ['stdout', 'stderr']:
            record[field] = record[field].strip()
    report = {'passed': True, 'records': records,
              'sha256': {name: hashlib.sha256(path.read_bytes()).hexdigest()
                         for name, path in [('native', args.native), ('oracle', args.oracle), ('input', args.input)]}}
    (args.output/'audit.json').write_text(json.dumps(report, indent=2)+'\n')
    print('Passed pipeline endpoint, startup and MD5-after-warmup audit')


if __name__ == '__main__':
    main()
