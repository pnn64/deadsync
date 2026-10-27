import hashlib,json,subprocess
from pathlib import Path
root=Path('/mnt/c/GitHub/deadsync')
out=Path('/tmp/deadlib-flac-memory-pilot')
out.mkdir(exist_ok=False)
native='/tmp/deadlib-flac-decode-unrolled-taps'
rust='/tmp/deadlib-flac-memory-rust/release/deadlib-flac-allocation-probe'
cpp='/tmp/deadlib-flac-memory-cpp/flac_decode_allocations'
records=[]
for prefix in ['01','43']:
 source=next((root/'target/deadlib-flac-reference/conformance/subset').glob(prefix+'*.flac'))
 index=out/(prefix+'.index')
 ogg=out/(prefix+'.oga')
 result=subprocess.run([native,'frames',str(source),'2',str(index),str(ogg)],capture_output=True,text=True,check=True)
 full=tuple(map(int,result.stdout.strip().split('\t')[3:5]))
 for exe,modes in [(rust,['frames','stream','stream-md5','symphonia-new-crc','ogg-stream','symphonia-new-ogg-crc']),
                   (cpp,['libflac-frames','libflac-new','libflac-new-md5','ffmpeg','libflac-new-ogg'])]:
  for mode in modes:
   command=[exe,mode,str(ogg if 'ogg' in mode and exe==cpp else source)]
   command+=([str(index),'2'] if exe==cpp else ['2'])
   result=subprocess.run(command,capture_output=True,text=True)
   assert not result.returncode,(command,result.stderr)
   assert tuple(map(int,result.stdout.strip().split('\t')[3:5]))==full,(command,result.stdout)
   cells=[list(map(int,line.split('\t')[1:])) for line in result.stderr.splitlines() if line.startswith('ALLOC\t')]
   assert len(cells)==3,(command,result.stderr)
   records.append(dict(case=source.name,mode=mode,phases=cells,command=command,
                       binary_sha256=hashlib.sha256(Path(exe).read_bytes()).hexdigest()))
   print(prefix,mode,cells[1],flush=True)
(out/'results.json').write_text(json.dumps(records,indent=2)+'\n')
