from pathlib import Path
import hashlib,json,os,sys
root=Path(__file__).resolve().parents[2]
base=root/'crates/deadlib-audio/decode/flac/benches/run.py'
build=root/'target/deadlib-flac-reference/windows-perf-build'
source=base.read_text()
source=source.replace('os.sched_setaffinity(0, {args.cpu})','set_windows_affinity(args.cpu)')
source=source.replace("Path('/proc/cpuinfo').read_text().split('\\n\\n')[0]","os.environ.get('PROCESSOR_IDENTIFIER', 'unknown Windows CPU')")
prefix='''def set_windows_affinity(cpu):
    import ctypes
    from ctypes import wintypes
    kernel=ctypes.WinDLL('kernel32',use_last_error=True)
    kernel.GetCurrentProcess.restype=wintypes.HANDLE
    kernel.SetProcessAffinityMask.argtypes=[wintypes.HANDLE,ctypes.c_size_t]
    kernel.SetProcessAffinityMask.restype=wintypes.BOOL
    if not kernel.SetProcessAffinityMask(kernel.GetCurrentProcess(),1<<cpu):
        raise ctypes.WinError(ctypes.get_last_error())
'''
source=prefix+'\n'+source
for label,cases,modes in [
 ('raw',['subset/01','subset/03','subset/06','subset/12','subset/14','subset/15','subset/28','subset/44','uncommon/08'],['frames','libflac-frames','symphonia-crc']),
 ('pipelines',['subset/01','subset/03','subset/15','subset/28','subset/44','uncommon/08'],['stream','stream-md5','libflac-new','libflac-new-md5','symphonia-new-crc','ogg-stream','ogg-stream-md5','libflac-new-ogg','libflac-new-ogg-md5','symphonia-new-ogg-crc'])]:
 out=build/('measure-'+label)
 sys.argv=[str(base),'--native',str(build/'decode.exe'),'--oracle',str(build/'oracle/Release/flac_decode_benchmark.exe'),'--corpus',str(root/'target/deadlib-flac-reference/conformance'),'--output',str(out),'--rounds','5','--milliseconds','100','--modes']+modes
 for case in cases:sys.argv+=['--case',case]
 exec(compile(source,str(Path(__file__)), 'exec'),{'__file__':str(base),'__name__':'__main__'})
 adapter={'adapter_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),'base_sha256':hashlib.sha256(base.read_bytes()).hexdigest(),
          'executed_source_sha256':hashlib.sha256(source.encode()).hexdigest(),'cpu':2,'affinity':'Windows SetProcessAffinityMask; children inherit the mask',
          'changes':'Only process affinity and CPU-description collection differ from benches/run.py.'}
 (out/'windows-adapter.json').write_text(json.dumps(adapter,indent=2)+'\n')
 (out/'executed-runner.py').write_text(source)
