"""Supply missing build-only Debian headers in the existing isolated prefix."""
from pathlib import Path
import hashlib, json, os, subprocess

prefix = Path('/var/tmp/deadsync-rust-adapters-native-deps')
download = prefix / 'migration-downloads'
download.mkdir(parents=True, exist_ok=True)
evidence = Path(__file__).resolve().parent
packages = ['libdbus-1-dev=1.16.2-2', 'libdbus-1-3=1.16.2-2',
            'libsystemd-dev=257.13-1~deb13u1', 'libsystemd0=257.13-1~deb13u1']
with (evidence / 'linux-native-deps.log').open('ab') as log:
    subprocess.run(['apt-get', 'download', *packages], cwd=download, stdout=log,
                   stderr=subprocess.STDOUT, check=True)
    records = []
    for archive in sorted(download.glob('*.deb')):
        subprocess.run(['dpkg-deb', '--extract', str(archive), str(prefix / 'root')],
                       stdout=log, stderr=subprocess.STDOUT, check=True)
        records.append(dict(file=archive.name, sha256=hashlib.sha256(archive.read_bytes()).hexdigest()))
    for name in ['dbus-1.pc', 'libsystemd.pc']:
        config = prefix / 'root/usr/lib/x86_64-linux-gnu/pkgconfig' / name
        config.write_text(config.read_text().replace('=/usr\n', f'={prefix}/root/usr\n')
                          .replace('=/usr/', f'={prefix}/root/usr/'))
    env = os.environ.copy()
    env['PKG_CONFIG_PATH'] = str(prefix / 'root/usr/lib/x86_64-linux-gnu/pkgconfig')
    subprocess.run(['pkg-config', '--libs', '--cflags', 'dbus-1'], env=env,
                   stdout=log, stderr=subprocess.STDOUT, check=True)
(evidence / 'linux-native-deps.json').write_text(json.dumps(dict(packages=packages, files=records,
    prefix=str(prefix / 'root'), scope='local extracted native build dependencies; no system installation'), indent=2)+'\n')
print(json.dumps(records))
