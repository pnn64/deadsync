"""Independent 3-bit IMA truth, encoder checks and explicit four-reader outcomes."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import shutil
import struct
import subprocess
import sys
import time
import zipfile
from ima3_fixtures import cases,pcm_source,encoded_truth
from reference_pcm import compare
from reference_support import build_dr_wav,build_symphonia


def sha(data):return hashlib.sha256(data).hexdigest()


def main():
    ap=argparse.ArgumentParser();ap.add_argument('--output',type=Path,required=True);args=ap.parse_args()
    root=Path(__file__).resolve().parents[1];workspace=root.parents[3];output=args.output.resolve();output.parent.mkdir(parents=True,exist_ok=True)
    for suffix in ('','.build.log','.corpus.zip','-corpus'):
        if Path(str(output)+suffix).exists():ap.error('choose a fresh evidence destination')
    corpus=Path(str(output)+'-corpus');corpus.mkdir()
    sys.path.insert(0,str(workspace/'target/wav/python'));import soundfile as sf
    dr,dr_meta=build_dr_wav(workspace,root);sym,sym_meta=build_symphonia(workspace,root)
    ffmpeg=shutil.which('ffmpeg')
    if not ffmpeg:raise RuntimeError('FFmpeg required for independently checked alternate arithmetic/packing')
    names=('adpcm','adpcm_file','adpcm_forward','encode_adpcm')
    sources=sorted((root/'src').rglob('*.rs'))+[root/f'examples/{n}.rs' for n in names]+[Path(__file__),root/'tests/ima3_fixtures.py',root/'tests/reference_pcm.py',root/'tests/pcm_fixtures.py',root/'tests/reference_support.py',root/'tests/oracle/dr_wav.c',root/'Cargo.toml']
    before={p.relative_to(root).as_posix():sha(p.read_bytes()) for p in sources}
    command=['cargo','build','--locked','-p','deadlib-audio-decode-wav','--target-dir','target/wav']
    for n in names:command+=['--example',n]
    process=subprocess.run(command,cwd=workspace,capture_output=True);Path(str(output)+'.build.log').write_bytes(process.stdout+process.stderr)
    if process.returncode:raise RuntimeError('native drivers failed to build')
    binaries={n:workspace/('target/wav/debug/examples/'+n+('.exe' if sys.platform=='win32' else '')) for n in names}
    results=[];manifest={}
    with zipfile.ZipFile(str(output)+'.corpus.zip','x',compression=zipfile.ZIP_DEFLATED,compresslevel=9) as archive:
        def save(name,payload):
            path=corpus/(name+'.wav');path.write_bytes(payload);manifest[path.name]=sha(payload)
            entry=zipfile.ZipInfo(path.name,date_time=(1980,1,1,0,0,0));entry.create_system=3;entry.external_attr=0o600<<16;archive.writestr(entry,payload,compress_type=zipfile.ZIP_DEFLATED,compresslevel=9)
            return path
        def check(info,payload,truth,multiply):
            path=save(info['name'],payload);channels=info['channels'];frames=info['frames'];ours={}
            commands={'borrowed':[binaries['adpcm'],path],'reader':[binaries['adpcm_file'],path],
                      'forward_7_width1':[binaries['adpcm_forward'],path,7,1],
                      'forward_127_width17':[binaries['adpcm_forward'],path,127,17]}
            for label,command in commands.items():
                process=subprocess.run([str(x) for x in command],capture_output=True);ours[label]=compare(process.stdout,truth,2,process.returncode,process.stderr)
            for start in sorted(set((frames//2,min(frames,1+(info['block_bytes']//channels-4)*8//3-1)))):
                count=min(17,frames-start);process=subprocess.run([str(binaries['adpcm_file']),str(path),str(start),str(count)],capture_output=True)
                ours[f'range_{start}']=compare(process.stdout,truth[start*channels*2:(start+count)*channels*2],2,process.returncode,process.stderr)
            references={}
            def reference(label,raw,code=0,stderr=b''):
                prefix=raw[:len(truth)]
                references[label]=dict(exit_code=code,full_bytes=len(raw),full_sha256=sha(raw),fact_trimmed_spec=compare(prefix,truth,2,code,stderr),fact_trimmed_combined=compare(prefix,multiply,2,code,stderr))
            for label,exe in (('dr_wav',dr),('symphonia',sym)):
                process=subprocess.run([str(exe),str(path)],capture_output=True);reference(label,process.stdout,process.returncode,process.stderr)
            process=subprocess.run([ffmpeg,'-v','error','-i',str(path),'-c:a','pcm_s16le','-f','s16le','-'],capture_output=True);reference('ffmpeg',process.stdout,process.returncode,process.stderr)
            try:
                decoded,rate=sf.read(str(path),dtype='int16',always_2d=True);reference('libsndfile',decoded.astype('<i2',copy=False).tobytes());references['libsndfile']['sample_rate']=rate
            except Exception as error:reference('libsndfile',b'',1,str(error).encode())
            # Predeclared independent packing check: FFmpeg uses combined-rounding
            # reconstruction. Production must match RIFFNEW's separate terms.
            layout_gate=info['container']=='RIFF' and channels<=2
            own=all(row['exact'] for row in ours.values());layout=not layout_gate or references['ffmpeg']['fact_trimmed_combined']['exact']
            results.append(dict(**info,fixture_sha256=sha(payload),truth_sha256=sha(truth),combined_truth_sha256=sha(multiply),ours=ours,references=references,own_pass=own,ffmpeg_layout_gate=layout_gate,ffmpeg_layout_pass=layout,passed=own and layout))
            if len(results)%20==0:print(f'{len(results)} cases; failures={sum(not r["passed"] for r in results)}',flush=True)
        for info,payload,truth,multiply in cases():check(info,payload,truth,multiply)
        for channels in (1,2,6,33):
            for groups in (1,3):
                block_bytes=channels*(4+12*groups)
                for frames in (1,17,1025):
                    for signal in ('constant','ramp','noise'):
                        name=f'encode-ima3-{channels}-g{groups}-{frames}-{signal}';source,samples=pcm_source(channels,frames,signal);path=save(name+'-source',source)
                        process=subprocess.run([str(binaries['encode_adpcm']),str(path),'ima3',str(block_bytes)],capture_output=True)
                        if process.returncode:raise RuntimeError(process.stderr.decode(errors='replace'))
                        payload=process.stdout;truth=encoded_truth(payload,channels,frames,block_bytes);multiply=encoded_truth(payload,channels,frames,block_bytes,True)
                        reconstructed=struct.unpack('<'+'h'*(len(truth)//2),truth);errors=[a-b for a,b in zip(samples,reconstructed)]
                        info=dict(name=name,container='RIFF',channels=channels,groups=groups,frames=frames,block_bytes=block_bytes,encoded=True,signal=signal,encoder_exit_code=process.returncode,mean_square_error=sum(e*e for e in errors)/len(errors),max_absolute_error=max(map(abs,errors)))
                        check(info,payload,truth,multiply)
    after={p.relative_to(root).as_posix():sha(p.read_bytes()) for p in sources}
    report=dict(timestamp_utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),platform=platform.platform(),rustc=subprocess.check_output(['rustc','-Vv'],text=True),sources=before,inputs_unchanged=before==after,binaries={n:sha(p.read_bytes()) for n,p in binaries.items()},dr_wav=dr_meta,symphonia=sym_meta,libsndfile=sf.__libsndfile_version__,soundfile=sf.__version__,ffmpeg=subprocess.check_output([ffmpeg,'-version'],text=True).splitlines()[0],ffmpeg_binary_sha256=sha(Path(ffmpeg).read_bytes()),corpus_sha256=sha(Path(str(output)+'.corpus.zip').read_bytes()),fixtures=manifest,passed=sum(r['passed'] for r in results),total=len(results),own_passed=sum(r['own_pass'] for r in results),layout_passed=sum(r['ffmpeg_layout_pass'] for r in results if r['ffmpeg_layout_gate']),layout_total=sum(r['ffmpeg_layout_gate'] for r in results),cases=results)
    output.write_text(json.dumps(report,indent=2)+'\n');print(f"{report['passed']}/{report['total']} gates; {report['layout_passed']}/{report['layout_total']} independent FFmpeg packing/combined-arithmetic gates",flush=True)
    if report['passed']!=report['total'] or before!=after:raise SystemExit(1)


if __name__=='__main__':main()
