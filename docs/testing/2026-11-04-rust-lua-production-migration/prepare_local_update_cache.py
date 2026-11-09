"""Restore execute permission on extensionless copied Cargo executables."""
from pathlib import Path
import json,stat
target=Path('/var/tmp/deadsync-song-rust-production-20261107')
assert target.resolve()==target and (target/'debug/.fingerprint').is_dir()
changed=0
for path in (target/'debug').rglob('*'):
    if path.is_file() and '.' not in path.name:
        path.chmod(path.stat().st_mode|stat.S_IXUSR)
        changed+=1
print(json.dumps(dict(target=str(target),extensionless_files_made_executable=changed)))
