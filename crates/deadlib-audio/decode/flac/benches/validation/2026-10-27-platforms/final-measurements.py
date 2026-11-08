from pathlib import Path,PurePosixPath
import json,subprocess,sys
root=Path(__file__).resolve().parents[2]
bench=root/'crates/deadlib-audio/decode/flac/benches'
win=root/'target/deadlib-flac-reference/windows-perf-build'
corpus=root/'target/deadlib-flac-reference/conformance'
winc=root/'target/deadlib-flac-reference/latency-windows-build/Release/flac_latency_benchmark.exe'
linuxroot=PurePosixPath('/mnt/c/GitHub/deadsync')
linuxbench=linuxroot/'crates/deadlib-audio/decode/flac/benches'
linuxcorpus=linuxroot/'target/deadlib-flac-reference/conformance'
linuxc=PurePosixPath('/home/perfecttaste/.cache/deadlib-flac-performance/latency-oracle/flac_latency_benchmark')
cases=['subset/01','subset/03','subset/28','subset/44','uncommon/08']
startupcases=['subset/01','subset/03','subset/06','subset/12','subset/14','subset/15','subset/28','subset/44','uncommon/08']
select=lambda values:[arg for c in values for arg in ['--case',c]]
commands=[
 [sys.executable,bench/'run.py','--native',win/'decode-latency.exe','--oracle',winc,'--corpus',corpus,'--output',win/'measure-broad','--modes','frames','libflac-frames','symphonia-crc','--rounds','5','--milliseconds','100'],
 [sys.executable,bench/'run_seek.py','--native',win/'decode-latency.exe','--oracle',winc,'--corpus',corpus,'--output',win/'measure-seek-warm','--rounds','5','--warm-index']+select(cases),
 ['wsl','-d','Debian','--','python3',linuxbench/'run.py','--native','/tmp/deadlib-flac-decode-latency','--oracle',linuxc,'--corpus',linuxcorpus,'--output','/tmp/deadlib-flac-final-startup','--modes','startup','libflac-new-startup','symphonia-new-startup','--rounds','5','--milliseconds','100']+select(startupcases),
 ['wsl','-d','Debian','--','python3',linuxbench/'run_seek.py','--native','/tmp/deadlib-flac-decode-latency','--oracle',linuxc,'--corpus',linuxcorpus,'--output','/tmp/deadlib-flac-final-seek-warm','--rounds','5','--warm-index']+select(cases),
 [sys.executable,bench/'check_oracle.py','--native',win/'decode-latency.exe','--oracle',winc,'--input',next((corpus/'subset').glob('01*flac')),'--output',win/'latency-oracle-audit'],
 ['wsl','-d','Debian','--','python3',linuxbench/'check_oracle.py','--native','/tmp/deadlib-flac-decode-latency','--oracle',linuxc,'--input',linuxcorpus/'subset/01 - blocksize 4096.flac','--output','/tmp/deadlib-flac-final-oracle-audit']
]
records=json.loads((Path(__file__).parent/'final-measurements.json').read_text())[:2] if '--resume' in sys.argv else []
for command in commands[len(records):]:
 print('RUN',command[1:3],flush=True)
 command=list(map(str,command))
 result=subprocess.run(command,cwd=root)
 records.append({'command':command,'exit_code':result.returncode})
 (Path(__file__).parent/'final-measurements.json').write_text(json.dumps(records,indent=2)+'\n',encoding='utf-8')
 assert not result.returncode,command
