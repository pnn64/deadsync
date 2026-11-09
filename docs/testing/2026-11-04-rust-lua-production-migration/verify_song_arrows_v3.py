"""Recheck ArrowEffects with the existing manager lifetime coverage included."""
import subprocess, sys
from pathlib import Path

d = Path(__file__).parent
base = [sys.executable, str(d / 'checks.py')]
tail = ['song', '--target-dir', '/var/tmp/deadsync-song-rust-production-20261107',
        '--features', 'rust-engine,test-support', '--jobs', '4']

def run(args, code=0):
    result = subprocess.run(args)
    assert result.returncode == code, (args, result.returncode, code)

targets = ['rust_statistics', 'rust_arrows', 'rust_clock', 'rust_catalog',
           'rust_game_state', 'rust_player_options', 'rust_player_state']
test_args = [value for name in targets for value in ['--test', name]]
run([*base, 'song-arrows-focus-v7', *tail, *test_args, '--no-fail-fast'])
run([*base, 'song-arrows-lint-v5', *tail, '--clippy'], 101)
run([*base, 'song-arrows-library-v3', *tail, '--lib', '--filter',
     'host::tests::arrow_', '--no-fail-fast'])
run([sys.executable, str(d / 'check_song_arrows_without_test_support.py'), '-v3'])
run([sys.executable, str(d / 'run_song_arrows_regressions.py'),
     'song-arrows-regressions-v3'])
run([sys.executable, str(d / 'review_song_arrows.py'),
     'song-arrows-regressions-v3-song-linux-debug',
     'song-arrows-focus-v7-song-linux-debug',
     'song-arrows-lint-v5-song-linux-debug',
     'song-arrows-library-v3-song-linux-debug'])
print('Final ArrowEffects and manager lifetime validation review passed.', flush=True)
