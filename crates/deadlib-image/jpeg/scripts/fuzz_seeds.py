"""Build the deterministic `decode` seed archive for fuzz_native.py.

Every fixture JPEG under tests/fixtures and every repository asset JPEG of at
most 256 KiB is stored as decode/<path with '/' replaced by '__'>, sorted, with
fixed timestamps, so the archive hash depends only on the inputs.
"""
from pathlib import Path
import sys
import zipfile

LIMIT = 262144


def main():
    repo = Path(__file__).resolve().parents[4]
    package = repo / 'crates/deadlib-image/jpeg'
    files = sorted(
        path for root in (package / 'tests/fixtures', repo / 'assets')
        for path in root.rglob('*') if path.is_file()
        and path.suffix.lower() in {'.jpg', '.jpeg'} and path.stat().st_size <= LIMIT)
    output = Path(sys.argv[1])
    with zipfile.ZipFile(output, 'w', zipfile.ZIP_DEFLATED) as archive:
        for path in files:
            name = 'decode/' + path.relative_to(repo).as_posix().replace('/', '__')
            info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            archive.writestr(info, path.read_bytes())
    print(f'{len(files)} seeds -> {output}')


if __name__ == '__main__':
    main()
