"""Archive fresh seed_fuzz inputs for one-file transfer to Linux.

The raw stream corpus also receives zlib seeds no larger than 64 KiB.
Stable ZIP metadata and literal UTF-8 filename sorting make output reproducible.
"""
from argparse import ArgumentParser
from pathlib import Path
import hashlib
import json
import zipfile


def main():
    parser = ArgumentParser(description=__doc__)
    parser.add_argument('--corpus-root', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    manifest = args.output.with_suffix('.json')
    assert not args.output.exists() and not manifest.exists()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    records = []
    with zipfile.ZipFile(args.output, 'w', compression=zipfile.ZIP_STORED) as archive:
        for target in ['png_stream', 'inflate_stream']:
            entries = [(path.name, path) for path in (args.corpus_root / target).iterdir()]
            if target == 'inflate_stream':
                entries += [('zlib-' + path.name, path) for path in (args.corpus_root / 'zlib').iterdir()
                            if path.stat().st_size <= 65536]
            assert len({name for name, _ in entries}) == len(entries)
            digest, sizes = hashlib.sha256(), []
            for name, path in sorted(entries, key=lambda entry: entry[0].encode()):
                data = path.read_bytes()
                assert len(data) <= 65536
                info = zipfile.ZipInfo(target + '/' + name, date_time=(1980, 1, 1, 0, 0, 0))
                info.create_system = 3
                info.external_attr = 0o100644 << 16
                archive.writestr(info, data)
                digest.update(name.encode() + b'\0' + data)
                sizes.append(len(data))
            records.append({'target': target, 'files': len(sizes), 'bytes': sum(sizes),
                            'maximum_file_bytes': max(sizes), 'sha256': digest.hexdigest()})
    metadata = {'archive_sha256': hashlib.sha256(args.output.read_bytes()).hexdigest(),
                'initial_corpora': records, 'runner_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                'hash_method': 'Ascending literal UTF-8 filename bytes, NUL, file contents'}
    manifest.write_text(json.dumps(metadata, indent=2) + '\n', encoding='utf-8', newline='\n')
    print(json.dumps(metadata))


if __name__ == '__main__':
    main()
