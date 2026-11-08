from pathlib import Path
import hashlib,json,struct,subprocess,shutil,math
repo=Path.cwd();root=repo/'target/deadlib-audio-reference';run=root/'eos-counter-wrap-before-after-verified-20261027';run.mkdir(exist_ok=False);sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest();snap=root/'eos-clamping-20261027/pre-overflow-source';source=root/'eos-clamping-selected-seeds-final-20261027';shutil.copyfile(source/'counter-wrap.ogg',run/'input.ogg');shutil.copyfile(source/'counter-wrap.out',run/'reference.out');w=(run/'reference.out').read_bytes();at=8;planes=[bytearray(),bytearray()]
while True:
 n=int.from_bytes(w[at:at+4],'little');at+=4
 if not n:break
 assert struct.unpack_from('<III',w,at)==(0,2,48000);at+=12
 for p in planes:p.extend(w[at:at+n*4]);at+=n*4
assert at==len(w)and len(planes[0])//4==1600
rows=[];snapshots={};driver=(root/'counter_wrap_driver.rs').read_bytes()
for variant in ['before','after']:
 folder=run/variant;(folder/'src').mkdir(parents=True);(folder/'src/main.rs').write_bytes(driver)
 if variant=='before':
  for package in ['ogg','vorbis']:
   target=folder/package;shutil.copytree(snap/package/'src',target/'src');manifest=(repo/f'crates/deadlib-audio/decode/{package}/Cargo.toml').read_text(encoding='utf8').split('\n[[bench]]')[0];(target/'Cargo.toml').write_text(manifest,encoding='utf8',newline='\n')
   for p in(target/'src').glob('*.rs'):snapshots[p.relative_to(folder).as_posix()]=sha(p)
  dependency=folder/'vorbis'
 else:dependency=repo/'crates/deadlib-audio/decode/vorbis'
 (folder/'Cargo.toml').write_text(f'[package]\nname="deadlib-counter-{variant}"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\ncodec={{package="deadlib-audio-decode-vorbis",path={json.dumps(dependency.as_posix())}}}\n',encoding='utf8',newline='\n')
 args=['cargo','build','--release','--offline','--manifest-path',str(folder/'Cargo.toml')]
 with(run/(variant+'-build.log')).open('wb')as f:r=subprocess.run(args,stdout=f,stderr=subprocess.STDOUT)
 assert r.returncode==0,(run/(variant+'-build.log')).read_text(encoding='utf8')
 binary=folder/f'target/release/deadlib-counter-{variant}.exe';r=subprocess.run([str(binary),str(run/'input.ogg')],capture_output=True);assert r.returncode==0,r.stderr;(run/(variant+'.out')).write_bytes(r.stdout);ok,frames,declared,emitted,channels=struct.unpack_from('<BQQQI',r.stdout);assert ok and channels==2 and declared==(1<<63)-1 and emitted==frames and frames==(586 if variant=='before'else 1600)
 at=29;peak=error=energy=0.;values=0
 for p in planes:
  for (x,),(y,) in zip(struct.iter_unpack('<f',r.stdout[at:at+frames*4]),struct.iter_unpack('<f',p[:frames*4])):
   assert math.isfinite(x)and math.isfinite(y);d=abs(x-y);peak=max(peak,abs(y));error=max(error,d);energy+=d*d;values+=1
  at+=frames*4
 assert at==len(r.stdout);rms=math.sqrt(energy/values);assert error<=3e-6*max(1,peak)and rms<=5e-7*max(1,peak)
 rows.append({'variant':variant,'command':args,'source_sha256':sha(folder/'src/main.rs'),'manifest_sha256':sha(folder/'Cargo.toml'),'binary_sha256':sha(binary),'output_sha256':sha(run/(variant+'.out')),'frames':frames,'declared':declared,'emitted':emitted,'values':values,'peak_error':error,'rms_error':rms,'reference_peak':peak})
summary={'scope':'Actual container witness for signed reference counter overflow. Both select explicit reference EOS. Before is the captured pre-wrap implementation; it emits 586 instead of the C 1600 frames. Corrected reference wrapping emits exactly 1600 with unchanged original PCM bounds.','input_sha256':sha(run/'input.ogg'),'reference_sha256':sha(run/'reference.out'),'before_source_snapshot_sha256':snapshots,'after_codec_source_sha256':{p.relative_to(repo).as_posix():sha(p)for part in ['ogg','vorbis']for p in(repo/f'crates/deadlib-audio/decode/{part}/src').glob('*.rs')},'results':rows}
(run/'checks.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf8',newline='\n');print([(r['variant'],r['frames'],r['declared'])for r in rows])
