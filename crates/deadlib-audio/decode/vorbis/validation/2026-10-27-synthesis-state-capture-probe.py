from pathlib import Path
import subprocess,json,hashlib
repo=Path.cwd();root=repo/'target/deadlib-audio-reference';run=root/'seek-reference-20261027';out=run/'captured-final';out.mkdir(exist_ok=False)
decode=repo/'crates/deadlib-audio/decode';source=root/'eos-clamping-validation-final-verified-20261027/eos-inputs'
ids=sorted({int(line.split('\t')[0]) for line in (decode/'vorbis/tests/fixtures/eos-seek-differences-v1.tsv').read_text().splitlines()})
known=set(ids);ids=sorted(set(ids+[0,90,180,181,185,190,225,270,314,430,746,1059,315,630,945,1259]))
jobs=[(f'eos-{i}',source/f'eos-seek-{i}.ogg',source/f'eos-seek-{i}.bin',False,i in known)for i in ids]
jobs += [(f'origin-{i}',run/f'origin-{i}.ogg',run/f'origin-{i}.bin',False,False) for i in [-576,-37,0,900]]
manifest=[]
for label,ogg,requests,floor0,diff in jobs:
 command=['python',str(decode/'vorbis/tests/oracle/replay_seek_trace.py'),'--input',str(ogg),'--requests',str(requests),'--oracle',str(root/'vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe'),'--trace',str(run/'trace-final/build/Release/deadlib_vorbis_seek_trace.exe'),'--output',str(out/label)]
 if floor0:command.append('--floor0')
 result=subprocess.run(command,capture_output=True,text=True);print(result.stdout.strip(),flush=True)
 assert result.returncode==0,(label,result.stderr)
 data=json.loads((out/label/'checks.json').read_text());data['known_difference']=diff;data['label']=label;manifest.append(data)
(out/'checks.json').write_text(json.dumps({'captures':manifest,'known_difference_streams':len(known),'operations':sum(x['operations']for x in manifest),'requests':sum(x['requests']for x in manifest),'chunks':sum(x['read_chunks']for x in manifest)},indent=2)+'\n',encoding='utf8',newline='\n')
