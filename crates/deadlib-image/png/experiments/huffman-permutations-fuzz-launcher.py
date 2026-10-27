from pathlib import Path
import hashlib, json, os, shutil, subprocess, time

root = Path('/mnt/c/GitHub/deadsync'); png = root / 'crates/deadlib-image/png'
assert shutil.which('rustup') == '/home/perfecttaste/.cargo/bin/rustup'
command = ['python3', str(png / 'scripts/fuzz_native.py'), '--cargo-fuzz', '/tmp/deadlib-image-linux-tools/bin/cargo-fuzz',
           '--seed-archive', '/tmp/deadlib-image-huffman-permutations-seeds-20261028-a.zip',
           '--target-dir', '/tmp/deadlib-image-huffman-permutations-asan-target-20261028-b',
           '--output-dir', '/tmp/deadlib-image-huffman-permutations-asan-20261028-b', '--runs', '100000']
log = png / 'experiments/huffman-permutations-fuzz-launch.log'
record = png / 'experiments/huffman-permutations-fuzz-launch.json'
assert not log.exists() and not record.exists()
start = time.monotonic()
with log.open('wb') as stream:
    result = subprocess.run(command, stdout=stream, stderr=subprocess.STDOUT, cwd=root)
sha = lambda p: hashlib.sha256(p.read_bytes()).hexdigest()
record.write_text(json.dumps(dict(command=command, login_shell_rustup=shutil.which('rustup'), path=os.environ['PATH'],
                  exit_code=result.returncode, wall_seconds=round(time.monotonic() - start, 3),
                  log=log.relative_to(png).as_posix(), log_sha256=sha(log), runner_sha256=sha(Path(__file__))), indent=2) + '\n')
print(log.read_text(errors='replace'), flush=True)
raise SystemExit(result.returncode)
