"""Verify the native/reference compiler policy from recorded passed arguments."""
import ast
import hashlib
import json
from pathlib import Path
import sys

run = Path(sys.argv[1])
records = {}
for path in (run / 'compiler-audit').glob('*.txt'):
    arguments = ast.literal_eval(path.read_text().split('arguments: ', 1)[1])
    if '--crate-name' not in arguments:
        continue
    crate = arguments[arguments.index('--crate-name') + 1]
    records.setdefault(crate, []).append({'audit_sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
                                         'passed_arguments': arguments})
for crate in ['deadlib_audio_decode_flac', 'deadlib_audio_decode_ogg', 'symphonia_bundle_flac']:
    assert crate in records, crate
    for record in records[crate]:
        arguments = record['passed_arguments']
        assert '-Zsanitizer=address' in arguments, crate
        assert '-Cdebug-assertions' in arguments, crate
        assert '-Cpasses=sancov-module' in arguments, crate
        assert ('overflow-checks=off' in arguments) == (crate == 'symphonia_bundle_flac'), crate
Path(sys.argv[2]).write_text(json.dumps(records, indent=2) + '\n')
print('Verified native checked arithmetic, ASan and feedback; reference-only release wrapping')
