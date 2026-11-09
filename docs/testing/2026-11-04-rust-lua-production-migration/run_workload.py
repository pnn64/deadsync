"""Run a preserved production harness with pinned song inputs and process RSS."""
from pathlib import Path
import argparse, hashlib, json, os, subprocess, time

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('name')
p.add_argument('kind', choices=['live', 'prepare'])
p.add_argument('binary', type=Path)
p.add_argument('simfile', type=Path)
p.add_argument('--frames', type=int, default=241)
p.add_argument('--iterations', type=int, default=3)
args = p.parse_args()
root = Path(__file__).resolve().parents[3]
out = Path(__file__).resolve().parent / args.name
out.mkdir(exist_ok=False)
binary, simfile = args.binary.resolve(), args.simfile.resolve()
assert binary.is_file(), binary
assert simfile.is_file(), simfile
def sha(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()
inputs = {p.relative_to(simfile.parent).as_posix(): sha(p)
          for p in sorted(simfile.parent.rglob('*')) if p.is_file()}
print(f'pinned {len(inputs)} files for {simfile}', flush=True)
(out / 'inputs.json').write_text(json.dumps(inputs, indent=2)+'\n')
report = out / 'output.json'
command = [str(binary), str(simfile), str(report), '--benchmark', str(args.frames)] if args.kind == 'live' else [
    str(binary), '--compile', str(simfile), '--report', str(report), '--iterations', str(args.iterations)]
env = os.environ.copy()
if os.name != 'nt':
    env['LD_LIBRARY_PATH'] = '/var/tmp/deadsync-rust-adapters-native-deps/root/usr/lib/x86_64-linux-gnu:' + env.get('LD_LIBRARY_PATH', '')
    import resource
    before_usage = resource.getrusage(resource.RUSAGE_CHILDREN)
started = time.monotonic()
memory = {}
with (out / 'run.log').open('wb') as log:
    if os.name == 'nt':
        import ctypes
        from ctypes import wintypes
        class Counters(ctypes.Structure):
            _fields_ = [('cb', wintypes.DWORD), ('PageFaultCount', wintypes.DWORD)] + [
                (name, ctypes.c_size_t) for name in ['PeakWorkingSetSize', 'WorkingSetSize',
                'QuotaPeakPagedPoolUsage', 'QuotaPagedPoolUsage', 'QuotaPeakNonPagedPoolUsage',
                'QuotaNonPagedPoolUsage', 'PagefileUsage', 'PeakPagefileUsage', 'PrivateUsage']]
        get_memory = ctypes.WinDLL('psapi', use_last_error=True).GetProcessMemoryInfo
        get_memory.argtypes = [wintypes.HANDLE, ctypes.POINTER(Counters), wintypes.DWORD]
        get_memory.restype = wintypes.BOOL
        result = subprocess.Popen(command, cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT)
        samples = 0
        while True:
            counters = Counters(cb=ctypes.sizeof(Counters))
            # Popen retains its owned Windows process handle through wait/poll.
            if get_memory(int(result._handle), ctypes.byref(counters), counters.cb):
                samples += 1
                memory['peak_rss_bytes'] = max(memory.get('peak_rss_bytes', 0), counters.PeakWorkingSetSize)
                memory['peak_commit_bytes'] = max(memory.get('peak_commit_bytes', 0), counters.PeakPagefileUsage)
                memory['sampled_peak_private_bytes'] = max(memory.get('sampled_peak_private_bytes', 0), counters.PrivateUsage)
            else:
                memory['last_memory_query_error'] = ctypes.get_last_error()
            if result.poll() is not None:
                break
            time.sleep(0.1)
        memory['memory_samples'] = samples
        memory['memory_method'] = 'Windows GetProcessMemoryInfo; OS high-water working set/commit and 100ms private-byte samples'
    else:
        result = subprocess.run(command, cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT)
record = dict(command=command, binary_sha256=sha(binary), simfile_sha256=sha(simfile),
              exit_code=result.returncode, seconds=time.monotonic()-started,
              inputs_sha256=sha(out / 'inputs.json'), log_sha256=sha(out / 'run.log'),
              limitations='Debug and concurrent-build runs establish functionality only; release timing comparisons require controlled paired runs. RSS includes the whole process, not just Lua.')
if os.name != 'nt':
    usage = resource.getrusage(resource.RUSAGE_CHILDREN)
    record.update(peak_rss_bytes=usage.ru_maxrss*1024,
                  user_seconds=usage.ru_utime-before_usage.ru_utime,
                  system_seconds=usage.ru_stime-before_usage.ru_stime)
else:
    record.update(memory)
record['inputs_unchanged'] = all(sha(simfile.parent/name) == expected for name,expected in inputs.items())
if report.exists():
    record['output_sha256'] = sha(report)
(out / 'result.json').write_text(json.dumps(record, indent=2)+'\n')
print(json.dumps(record), flush=True)
print((out / 'run.log').read_text(errors='replace')[-4000:], flush=True)
assert record['inputs_unchanged']
raise SystemExit(result.returncode)
