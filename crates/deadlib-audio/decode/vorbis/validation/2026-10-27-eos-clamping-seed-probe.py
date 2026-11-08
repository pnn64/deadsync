from pathlib import Path
import hashlib,json,subprocess,struct,shutil
repo=Path.cwd();root=repo/'target/deadlib-audio-reference';run=root/'eos-clamping-seeds-verified-20261027';run.mkdir(exist_ok=False)
prior=root/'fuzz-floor1-guards-20261027/corpus'
for name in ['vorbis_headers','vorbis_packets','vorbis_stream','vorbis_encode']:shutil.copytree(prior/name,run/name,dirs_exist_ok=True)
sha=lambda b:hashlib.sha256(b).hexdigest();exe=root/'vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe';seeds=[];probes=[]
for i in range(1260):
 p=root/'eos-clamping-20261027/probe-inputs'/f'eos-stream-{i}.ogg';b=p.read_bytes();r=subprocess.run([str(exe),'oggstream',str(p)],capture_output=True);assert r.returncode==0,r.stderr;assert r.stdout==p.with_suffix('.out').read_bytes()
 out=r.stdout;at=8;frames=0;channels=int.from_bytes(out[:4],'little')
 while True:
  n=int.from_bytes(out[at:at+4],'little');at+=4
  if not n:break
  link,ch,rate=struct.unpack_from('<III',out,at);at+=12;assert link==0 and ch==channels;frames+=n;at+=n*ch*4
 assert at==len(out)
 for control in [0,128,130,255]:
  data=bytes([control])+b;assert len(data)<=131073;name=f'eos-{i}-control-{control}.bin';(run/'vorbis_stream'/name).write_bytes(data);seeds.append({'path':'vorbis_stream/'+name,'sha256':sha(data),'bytes':len(data),'case':i,'control':control})
 probes.append({'case':i,'source':p.relative_to(repo).as_posix(),'ogg_sha256':sha(b),'reference_response_sha256':sha(out),'frames':frames,'channels':channels})
(run/'targeted.json').write_text(json.dumps({'scope':'All 1260 known nonnegative EOS layouts, freshly recaptured with unseekable libvorbisfile. Controls select strict/reference policies, endpoint repair exclusion, pull/push/reset and both indexed prerolls. Declared/emitted count differences remain observable. Inherits prior corpus unchanged.','reference_binary_sha256':sha(exe.read_bytes()),'probes':probes,'seeds':seeds},indent=2)+'\n',encoding='utf8',newline='\n')
print('Prepared',len(seeds),'EOS seeds from',len(probes),'C-checked streams.')
