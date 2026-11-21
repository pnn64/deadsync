import subprocess
import sys
from pathlib import Path

path = Path(sys.argv[1])
with path.open('xb') as log:
    result = subprocess.run(sys.argv[2:], stdout=log, stderr=subprocess.STDOUT)
print(f'{path}: exit={result.returncode}')
print(path.read_text(encoding='utf-8', errors='replace')[-7000:])
sys.exit(result.returncode)
