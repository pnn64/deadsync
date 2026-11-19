"""Run one qualification process after compilation and validation are idle."""
import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('run', choices=('rgba8-run1', 'rgba8-run2', 'raw'))
args = parser.parse_args()
work = Path(__file__).resolve().parent
root = work.parents[1]
stem = f'final-{args.run}'
output = work / f'{stem}.tsv'
assert not output.exists(), f'{output} already exists; do not overwrite measurements'

def compilers():
    command = "$items = @(Get-Process | Where-Object { $_.ProcessName -in @('rustc','link','cargo','clippy-driver') } | Select-Object ProcessName,Id,CPU); ConvertTo-Json -InputObject $items -Compress"
    return json.loads(subprocess.check_output(['powershell.exe', '-NoProfile', '-Command', command], text=True))

before = compilers()
assert not before, f'Compilation is active; defer this run: {before}'
command = [str(work / 'final-timing.exe')]
if args.run.startswith('rgba8'):
    command.append('--rgba8')
command += ['target/png-decode-corpus-20261117', 'target/png-kodak-20261117', 'target/png-perf-20261119/assets']
environment = os.environ.copy()
environment['PNG_BENCH_MS'] = '40' if args.run == 'raw' else '80'
record = {
    'command': command,
    'working_directory': str(root),
    'PNG_BENCH_MS': environment['PNG_BENCH_MS'],
    'compiler_processes_before': before,
    'started_utc': datetime.now(timezone.utc).isoformat(),
}
with output.open('wb') as stdout, (work / f'{stem}-stderr.log').open('wb') as stderr:
    process = subprocess.run(command, cwd=root, env=environment, stdout=stdout, stderr=stderr)
record['finished_utc'] = datetime.now(timezone.utc).isoformat()
record['exit_code'] = process.returncode
record['compiler_processes_after'] = compilers()
(work / f'{stem}-process.json').write_text(json.dumps(record, indent=2) + '\n', encoding='utf-8')
print(json.dumps(record))
raise SystemExit(process.returncode)
