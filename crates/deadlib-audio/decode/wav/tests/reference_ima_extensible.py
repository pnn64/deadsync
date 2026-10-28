"""Extensible IMA: independent bit truth, prior-driver rejection and four readers."""
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
from ima_extensible_fixtures import cases,pcm_source,encoded_truth,wave,descriptor
from reference_pcm import compare
from reference_support import build_dr_wav,build_symphonia


def sha(data):return hashlib.sha256(data).hexdigest()


def main():
    ap=argparse.ArgumentParser();ap.add_argument('--output',type=Path,required=True);ap.add_argument('--before-binary',type=Path,help='optional exact retained prior driver, for before/after evidence');args=ap.parse_args()
    root=Path(__file__).resolve().parents[1];workspace=root.parents[3];output=args.output.resolve();output.parent.mkdir(parents=True,exist_ok=True)
    for suffix in ('','.build.log','.corpus.zip','-corpus'):
        if Path(str(output)+suffix).exists():ap.error('choose a new evidence destination')
    corpus=Path(str(output)+'-corpus');corpus.mkdir()
    baseline=root/'validation/bw64-dummy-adpcm-forward.json';baseline_report=json.loads(baseline.read_text())
    if args.before_binary:assert sha(args.before_binary.read_bytes())==baseline_report['binary_sha256']
    sys.path.insert(0,str(workspace/'target/wav/python'));import soundfile as sf
    dr,dr_meta=build_dr_wav(workspace,root);sym,sym_meta=build_symphonia(workspace,root);ffmpeg=shutil.which('ffmpeg');assert ffmpeg
    names=('adpcm','adpcm_file','adpcm_forward','encode_adpcm')
    paths=sorted((root/'src').rglob('*.rs'))+[root/f'examples/{n}.rs' for n in names]+[Path(__file__),root/'tests/ima_extensible_fixtures.py',root/'tests/ima3_fixtures.py',root/'tests/reference_pcm.py',root/'tests/pcm_fixtures.py',root/'tests/reference_support.py',root/'tests/oracle/dr_wav.c',root/'Cargo.toml']
    before={p.relative_to(root).as_posix():sha(p.read_bytes()) for p in paths}
    command=['cargo','build','--locked','-p','deadlib-audio-decode-wav','--target-dir','target/wav']
    for n in names:command+=['--example',n]
    process=subprocess.run(command,cwd=workspace,capture_output=True);Path(str(output)+'.build.log').write_bytes(process.stdout+process.stderr)
    if process.returncode:raise RuntimeError('native driver build failed')
    bins={n:workspace/('target/wav/debug/examples/'+n+('.exe' if sys.platform=='win32' else '')) for n in names};results=[];manifest={}
    with zipfile.ZipFile(str(output)+'.corpus.zip','x',compression=zipfile.ZIP_DEFLATED,compresslevel=9) as archive:
        def save(name,payload):
            path=corpus/(name+'.wav');assert not path.exists();path.write_bytes(payload);manifest[path.name]=sha(payload)
            info=zipfile.ZipInfo(path.name,date_time=(1980,1,1,0,0,0));info.create_system=3;info.external_attr=0o600<<16;archive.writestr(info,payload,compress_type=zipfile.ZIP_DEFLATED,compresslevel=9)
            return path
        def external(path,truth,combined):
            rows={}
            def record(label,raw,code,stderr=b''):
                rows[label]=dict(exit_code=code,full_bytes=len(raw),full_sha256=sha(raw),spec=compare(raw[:len(truth)],truth,2,code,stderr),combined=compare(raw[:len(combined)],combined,2,code,stderr))
            for label,exe in [('dr_wav',dr),('symphonia',sym)]:
                p=subprocess.run([str(exe),str(path)],capture_output=True);record(label,p.stdout,p.returncode,p.stderr)
            p=subprocess.run([ffmpeg,'-v','error','-i',str(path),'-c:a','pcm_s16le','-f','s16le','-'],capture_output=True);record('ffmpeg',p.stdout,p.returncode,p.stderr)
            try:
                data,rate=sf.read(str(path),dtype='int16',always_2d=True);record('libsndfile',data.astype('<i2',copy=False).tobytes(),0);rows['libsndfile']['sample_rate']=rate
            except Exception as e:record('libsndfile',b'',1,str(e).encode())
            return rows
        def check(info,payload,truth,combined,flat):
            path=save(info['name'],payload);frames=info['frames'];channels=info['channels'];ours={}
            commands={'borrowed':[bins['adpcm'],path],'reader':[bins['adpcm_file'],path],
                'forward_7_width1':[bins['adpcm_forward'],path,7,1,frames],
                'forward_127_width17':[bins['adpcm_forward'],path,127,17,frames]}
            for label,command in commands.items():
                p=subprocess.run([str(x) for x in command],capture_output=True);ours[label]=compare(p.stdout,truth,2,p.returncode,p.stderr)
            for start in sorted(set((0,min(frames,info['block_frames']-1),max(0,frames-2),frames))):
                count=min(17,frames-start);p=subprocess.run([str(bins['adpcm_file']),str(path),str(start),str(count)],capture_output=True);ours[f'range_{start}']=compare(p.stdout,truth[start*channels*2:(start+count)*channels*2],2,p.returncode,p.stderr)
            prior=None
            if args.before_binary and not info['trailing']:
                p=subprocess.run([str(args.before_binary.resolve()),str(path),'7','1'],capture_output=True)
                prior=dict(exit_code=p.returncode,output_bytes=len(p.stdout),stderr=p.stderr.decode(errors='replace')[:1500],rejected_for_extensible=p.returncode!=0 and not p.stdout and b'extensible' in p.stderr.lower())
            references=external(path,truth,combined)
            # Declared before execution: direct RIFF 4-bit mono/stereo dr_wav
            # parity; paired legacy 3-bit FFmpeg checks under its alternate rounding.
            core=info['container']=='RIFF' and channels<=2 and frames>0
            direct_gate=core and info['bits']==4;flat_gate=core and info['bits']==3;paired=None
            if flat_gate:
                paired_path=save(info['name']+'-legacy-pair',flat);p=subprocess.run([ffmpeg,'-v','error','-i',str(paired_path),'-c:a','pcm_s16le','-f','s16le','-'],capture_output=True)
                paired=dict(fixture_sha256=sha(flat),combined=compare(p.stdout[:len(combined)],combined,2,p.returncode,p.stderr))
            passed=all(row['exact'] for row in ours.values()) and (prior is None or prior['rejected_for_extensible']) and (not direct_gate or references['dr_wav']['spec']['exact']) and (not flat_gate or paired['combined']['exact'])
            results.append(dict(**info,fixture_sha256=sha(payload),truth_sha256=sha(truth),combined_truth_sha256=sha(combined),ours=ours,prior=prior,references=references,direct_dr_wav_gate=direct_gate,paired_ffmpeg_gate=flat_gate,paired=paired,passed=passed))
            if len(results)%40==0:print(f'{len(results)} cases; failures={sum(not r["passed"] for r in results)}',flush=True)
        for info,payload,truth,combined,flat in cases():check(info,payload,truth,combined,flat)
        for bits in (3,4):
            for channels in (1,2,6,33):
                block_bytes=channels*(40 if bits==3 else 16);block_frames=1+(block_bytes//channels-4)*8//bits
                for frames in (1,129):
                    for signal in ('constant','ramp','noise'):
                        base=f'encode-ext-{bits}-{channels}-{frames}-{signal}';source,samples=pcm_source(channels,frames,signal);source_path=save(base+'-source',source);first_audio=None
                        for mode in ('extensible','extensible-known'):
                            p=subprocess.run([str(bins['encode_adpcm']),str(source_path),'ima3' if bits==3 else 'ima',str(block_bytes),mode],capture_output=True)
                            assert p.returncode==0,p.stderr.decode(errors='replace');payload=p.stdout;truth,combined,audio=encoded_truth(payload,bits,channels,block_bytes,frames)
                            if first_audio is None:first_audio=audio
                            else:assert first_audio==audio
                            recovered=struct.unpack('<'+'h'*(len(truth)//2),truth);errors=[a-b for a,b in zip(samples,recovered)]
                            info=dict(name=base+'-'+mode,container='RIFF',bits=bits,channels=channels,block_bytes=block_bytes,block_frames=block_frames,frames=frames,mask=0,trailing=False,encoded=True,signal=signal,mode=mode,mean_square_error=sum(e*e for e in errors)/len(errors),max_absolute_error=max(map(abs,errors)))
                            flat=wave('RIFF',descriptor(bits,channels,block_bytes,0,extensible=False),audio,frames)
                            check(info,payload,truth,combined,flat)
    after={p.relative_to(root).as_posix():sha(p.read_bytes()) for p in paths}
    report=dict(timestamp_utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),platform=platform.platform(),rustc=subprocess.check_output(['rustc','-Vv'],text=True),sources=before,inputs_unchanged=before==after,binaries={n:sha(p.read_bytes()) for n,p in bins.items()},baseline_report_sha256=sha(baseline.read_bytes()),baseline_binary_sha256=baseline_report['binary_sha256'] if args.before_binary else None,dr_wav=dr_meta,symphonia=sym_meta,libsndfile=sf.__libsndfile_version__,soundfile=sf.__version__,ffmpeg=subprocess.check_output([ffmpeg,'-version'],text=True).splitlines()[0],ffmpeg_binary_sha256=sha(Path(ffmpeg).read_bytes()),corpus_sha256=sha(Path(str(output)+'.corpus.zip').read_bytes()),fixtures=manifest,passed=sum(r['passed'] for r in results),total=len(results),own_checks=sum(len(r['ours']) for r in results),prior_rejections=sum(r['prior'] is not None and r['prior']['rejected_for_extensible'] for r in results),direct_dr_wav_gates=sum(r['direct_dr_wav_gate'] for r in results),paired_ffmpeg_gates=sum(r['paired_ffmpeg_gate'] for r in results),cases=results)
    output.write_text(json.dumps(report,indent=2)+'\n');print(f"{report['passed']}/{report['total']} gates; {report['own_checks']} native checks; {report['prior_rejections']} previous-driver rejections",flush=True)
    if report['passed']!=report['total'] or before!=after:raise SystemExit(1)


if __name__=='__main__':main()
