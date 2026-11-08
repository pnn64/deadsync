"""Host adapters; affinity is inherited by benchmark child processes."""
import os
from pathlib import Path


def set_affinity(cpu):
    if os.name != 'nt':
        os.sched_setaffinity(0, {cpu})
        return
    import ctypes
    from ctypes import wintypes
    kernel = ctypes.WinDLL('kernel32', use_last_error=True)
    kernel.GetCurrentProcess.restype = wintypes.HANDLE
    kernel.SetProcessAffinityMask.argtypes = [wintypes.HANDLE, ctypes.c_size_t]
    kernel.SetProcessAffinityMask.restype = wintypes.BOOL
    if not kernel.SetProcessAffinityMask(kernel.GetCurrentProcess(), 1 << cpu):
        raise ctypes.WinError(ctypes.get_last_error())


def cpu_info():
    if os.name == 'nt':
        return os.environ.get('PROCESSOR_IDENTIFIER', 'unknown Windows CPU')
    return Path('/proc/cpuinfo').read_text().split('\n\n')[0]
