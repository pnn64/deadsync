"""Read local PDB function symbols for the frozen RGBA4444 binary; never execute it."""
import ctypes
from ctypes import wintypes as w
import json
from pathlib import Path

task = Path(__file__).resolve().parent
directory = task / "assembly-candidate2"
exe = directory / "bmp-codec-bench.exe"
kernel = ctypes.WinDLL("kernel32", use_last_error=True)
dbg = ctypes.WinDLL("dbghelp", use_last_error=True)
kernel.GetCurrentProcess.restype = w.HANDLE
process = kernel.GetCurrentProcess()


class Symbol(ctypes.Structure):
    _fields_ = [
        ("SizeOfStruct", w.ULONG), ("TypeIndex", w.ULONG),
        ("Reserved", ctypes.c_ulonglong * 2), ("Index", w.ULONG), ("Size", w.ULONG),
        ("ModBase", ctypes.c_ulonglong), ("Flags", w.ULONG),
        ("Value", ctypes.c_ulonglong), ("Address", ctypes.c_ulonglong),
        ("Register", w.ULONG), ("Scope", w.ULONG), ("Tag", w.ULONG),
        ("NameLen", w.ULONG), ("MaxNameLen", w.ULONG), ("Name", ctypes.c_char * 1),
    ]


assert ctypes.sizeof(Symbol) == 88 and Symbol.Name.offset == 84
Callback = ctypes.WINFUNCTYPE(w.BOOL, ctypes.POINTER(Symbol), w.ULONG, ctypes.c_void_p)
dbg.SymSetOptions.argtypes = [w.DWORD]
dbg.SymSetOptions.restype = w.DWORD
dbg.SymInitializeW.argtypes = [w.HANDLE, w.LPCWSTR, w.BOOL]
dbg.SymInitializeW.restype = w.BOOL
dbg.SymLoadModuleExW.argtypes = [w.HANDLE, w.HANDLE, w.LPCWSTR, w.LPCWSTR, ctypes.c_ulonglong, w.DWORD, ctypes.c_void_p, w.DWORD]
dbg.SymLoadModuleExW.restype = ctypes.c_ulonglong
dbg.SymEnumSymbols.argtypes = [w.HANDLE, ctypes.c_ulonglong, ctypes.c_char_p, Callback, ctypes.c_void_p]
dbg.SymEnumSymbols.restype = w.BOOL
dbg.SymCleanup.argtypes = [w.HANDLE]
dbg.SymCleanup.restype = w.BOOL
# Local explicit path, exact PDB matching, ignore the ambient symbol-server path.
dbg.SymSetOptions(0x2 | 0x4 | 0x200 | 0x400 | 0x1000 | 0x80000)
if not dbg.SymInitializeW(process, str(directory), False):
    raise ctypes.WinError(ctypes.get_last_error())
selected = []
total = 0


@Callback
def collect(pointer, size, context):
    global total
    total += 1
    symbol = pointer.contents
    name = ctypes.string_at(ctypes.addressof(symbol) + Symbol.Name.offset, symbol.NameLen).decode("utf-8", "replace")
    if "deadlib_image_bmp" in name:
        selected.append({"name": name, "address": hex(symbol.Address), "size": symbol.Size, "flags": symbol.Flags, "tag": symbol.Tag})
    return True


try:
    base = dbg.SymLoadModuleExW(process, None, str(exe), None, 0, 0, None, 0)
    if not base:
        raise ctypes.WinError(ctypes.get_last_error())
    if not dbg.SymEnumSymbols(process, base, b"*", collect, None):
        raise ctypes.WinError(ctypes.get_last_error())
finally:
    dbg.SymCleanup(process)
result = {"executable": str(exe), "symbol_count": total, "bmp_symbols": selected}
with (directory / "symbols.json").open("x", encoding="utf-8", newline="\n") as output:
    json.dump(result, output, indent=2)
    output.write("\n")
print(json.dumps({"symbol_count": total, "bmp_symbol_count": len(selected), "targets": [item for item in selected if any(term in item["name"] for term in ("masked_row", "map_row", "decode_with_limits"))]}, indent=2))

