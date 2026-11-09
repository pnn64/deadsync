"""Recheck the ownership correction, retaining every prior coverage target."""
import subprocess, sys
from pathlib import Path

d = Path(__file__).parent
base = [sys.executable, str(d / 'checks.py')]
tail = ['song', '--target-dir', '/var/tmp/deadsync-song-rust-production-20261107',
        '--features', 'rust-engine,test-support', '--jobs', '4']

def run(args, code=0):
    result = subprocess.run(args)
    assert result.returncode == code, (args, result.returncode, code)

run([*base, 'song-arrows-focus-v6', *tail, '--test', 'rust_statistics',
     '--test', 'rust_arrows', '--test', 'rust_clock', '--no-fail-fast'])
run([*base, 'song-arrows-lint-v4', *tail, '--clippy'], 101)
run([*base, 'song-arrows-library-v2', *tail, '--lib', '--filter',
     'host::tests::arrow_', '--no-fail-fast'])
run([sys.executable, str(d / 'check_song_arrows_without_test_support.py'), '-v2'])
run([sys.executable, str(d / 'run_song_arrows_regressions.py'),
     'song-arrows-regressions-v2'])
run([sys.executable, str(d / 'review_song_arrows.py'),
     'song-arrows-regressions-v2-song-linux-debug',
     'song-arrows-focus-v6-song-linux-debug',
     'song-arrows-lint-v4-song-linux-debug',
     'song-arrows-library-v2-song-linux-debug'])
print('Final ArrowEffects ownership correction and validation review passed.', flush=True)
