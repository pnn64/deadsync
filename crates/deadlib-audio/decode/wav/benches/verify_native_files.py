"""Verify native benchmark bytes and Windows file allocation/attribute evidence."""
import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path


class Standard(ctypes.Structure):
    _fields_=[('allocation',ctypes.c_int64),('end',ctypes.c_int64),('links',ctypes.c_uint32),('delete_pending',ctypes.c_ubyte),('directory',ctypes.c_ubyte)]


def main():
    ap=argparse.ArgumentParser();ap.add_argument('--report',type=Path,required=True);ap.add_argument('--fixtures',type=Path,required=True);ap.add_argument('--output',type=Path,required=True);args=ap.parse_args()
    if args.output.exists():ap.error('preserve evidence; choose a new output')
    report=json.loads(args.report.read_text());cases={x['case']['name']:x['case'] for x in report['runs']+report['memory_probes']};rows=[]
    if os.name=='nt':
        k=ctypes.WinDLL('kernel32',use_last_error=True)
        k.CreateFileW.argtypes=[ctypes.c_wchar_p,ctypes.c_uint32,ctypes.c_uint32,ctypes.c_void_p,ctypes.c_uint32,ctypes.c_uint32,ctypes.c_void_p];k.CreateFileW.restype=ctypes.c_void_p
        k.GetFileInformationByHandleEx.argtypes=[ctypes.c_void_p,ctypes.c_int,ctypes.c_void_p,ctypes.c_uint32];k.GetFileInformationByHandleEx.restype=ctypes.c_int
        k.CloseHandle.argtypes=[ctypes.c_void_p];k.CloseHandle.restype=ctypes.c_int
        k.GetFileAttributesW.argtypes=[ctypes.c_wchar_p];k.GetFileAttributesW.restype=ctypes.c_uint32
    for name,case in cases.items():
        path=(args.fixtures/(name+'.wav')).resolve();digest=hashlib.sha256()
        with path.open('rb') as f:
            for b in iter(lambda:f.read(1<<20),b''):digest.update(b)
        assert digest.hexdigest()==case['fixture_sha256'],name
        row=dict(name=name,file_bytes=path.stat().st_size,fixture_sha256=digest.hexdigest())
        if os.name=='nt':
            handle=k.CreateFileW(str(path),0x80000000,7,None,3,0x80,None)
            if handle==ctypes.c_void_p(-1).value:raise OSError(ctypes.get_last_error(),'CreateFileW')
            info=Standard()
            try:
                if not k.GetFileInformationByHandleEx(handle,1,ctypes.byref(info),ctypes.sizeof(info)):raise OSError(ctypes.get_last_error(),'GetFileInformationByHandleEx')
            finally:k.CloseHandle(handle)
            attributes=k.GetFileAttributesW(str(path))
            if attributes==0xffffffff:raise OSError(ctypes.get_last_error(),'GetFileAttributesW')
            row.update(allocation_bytes=info.allocation,end_of_file=info.end,links=info.links,file_attributes=attributes,sparse=bool(attributes&0x200),compressed=bool(attributes&0x800),directory=bool(info.directory),delete_pending=bool(info.delete_pending))
            assert info.end==row['file_bytes'] and info.allocation>=info.end and not row['sparse'] and not row['compressed'] and not row['directory']
        rows.append(row)
    args.output.write_text(json.dumps(dict(report_sha256=hashlib.sha256(args.report.read_bytes()).hexdigest(),verifier_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),platform=os.name,passed=len(rows),files=rows,scope='FILE_STANDARD_INFO AllocationSize/EndOfFile and file attributes, plus complete streaming SHA-256; benchmark allocated_file_bytes is GetCompressedFileSizeW storage length, not cluster-rounded AllocationSize'),indent=2)+'\n')
    print(f'{len(rows)} exact file hashes and storage records passed')


if __name__=='__main__':main()
