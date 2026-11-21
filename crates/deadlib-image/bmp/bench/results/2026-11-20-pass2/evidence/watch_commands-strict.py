"""Run command arrays while recording other compute processes every two seconds."""
from pathlib import Path
import ctypes
from ctypes import wintypes as w
import datetime
import json
import os
import subprocess
import sys
import time

OUT = Path(__file__).resolve().parent
ROOT = OUT.parents[1]
sys.stdout.reconfigure(encoding='utf-8')
kernel = ctypes.WinDLL('kernel32', use_last_error=True)


class Entry(ctypes.Structure):
    _fields_ = [('size', w.DWORD), ('usage', w.DWORD), ('pid', w.DWORD),
                ('heap', ctypes.c_size_t), ('module', w.DWORD), ('threads', w.DWORD),
                ('parent', w.DWORD), ('priority', w.LONG), ('flags', w.DWORD),
                ('name', w.WCHAR * 260)]


kernel.CreateToolhelp32Snapshot.argtypes = [w.DWORD, w.DWORD]
kernel.CreateToolhelp32Snapshot.restype = w.HANDLE
kernel.Process32FirstW.argtypes = [w.HANDLE, ctypes.POINTER(Entry)]
kernel.Process32FirstW.restype = w.BOOL
kernel.Process32NextW.argtypes = [w.HANDLE, ctypes.POINTER(Entry)]
kernel.Process32NextW.restype = w.BOOL
kernel.CloseHandle.argtypes = [w.HANDLE]
kernel.CloseHandle.restype = w.BOOL
COMPILERS = {'cargo.exe', 'rustc.exe', 'rustdoc.exe', 'clippy-driver.exe', 'cl.exe', 'clang.exe', 'clang-cl.exe', 'link.exe', 'lld.exe', 'lld-link.exe', 'ninja.exe', 'cmake.exe'}
PATTERNS = ('decode-bench', 'baseline', 'refine', 'ac-support', 'candidate',
            'native_snapshot', 'hotpath_perf', 'parity', 'optimization_edges',
            'pump_modes', 'genre_metadata', 'deadlib_image',
            'bench', 'timing', 'snapshot', 'rssp')


def processes():
    handle = kernel.CreateToolhelp32Snapshot(2, 0)
    if handle == ctypes.c_void_p(-1).value:
        raise ctypes.WinError(ctypes.get_last_error())
    result = []
    try:
        entry = Entry()
        entry.size = ctypes.sizeof(entry)
        found = kernel.Process32FirstW(handle, ctypes.byref(entry))
        if not found:
            raise ctypes.WinError(ctypes.get_last_error())
        while found:
            result.append(dict(pid=entry.pid, parent=entry.parent, name=entry.name))
            found = kernel.Process32NextW(handle, ctypes.byref(entry))
    finally:
        kernel.CloseHandle(handle)
    return result


import argparse
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('commands_json', type=Path)
parser.add_argument('--log', required=True, type=Path)
args = parser.parse_args()
runs = json.loads(args.commands_json.read_text(encoding='utf-8-sig'))
assert runs and all(isinstance(job.get('command'), list) and job['command']
                    and all(isinstance(arg, str) for arg in job['command']) for job in runs)
with args.log.open('x', encoding='utf-8') as log:
    def check(run, child=None):
        current = processes()
        own = {child.pid} if child else set()
        while True:
            more = {p['pid'] for p in current if p['parent'] in own}
            if more <= own:
                break
            own |= more
        bad = [p for p in current if p['pid'] not in own and
               (p['name'].lower() in COMPILERS or
                any(pattern in p['name'].lower() for pattern in PATTERNS))]
        log.write(json.dumps(dict(utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
                                 run=run, own_pids=sorted(own), external=bad)) + '\n')
        log.flush()
        if bad:
            if child and child.poll() is None:
                subprocess.run(['taskkill', '/PID', str(child.pid), '/T', '/F'], check=True,
                               stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
                child.wait()
            raise SystemExit('CONTAMINATED ' + run + ': ' + json.dumps(bad))

    for job in runs:
        name = job['name']
        check(name)
        print('START', name, flush=True)
        child = subprocess.Popen(job['command'], cwd=job.get('cwd', str(ROOT)))
        while child.poll() is None:
            check(name, child)
            try:
                child.wait(timeout=2)
            except subprocess.TimeoutExpired:
                pass
        check(name)
        if child.returncode:
            raise SystemExit(child.returncode)
print('COMPLETE', args.commands_json, 'all monitored commands', flush=True)
