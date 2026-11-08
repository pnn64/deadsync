"""Independent compressed wavl ranges and four-reader capability observations."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import time
import zipfile
from adpcm_wavl_fixtures import cases
from reference_pcm import compare
from reference_support import build_dr_wav,build_symphonia


def sha(data):return hashlib.sha256(data).hexdigest()


def main():
    ap=argparse.ArgumentParser();ap.add_argument('--output',type=Path,required=True);args=ap.parse_args()
    root=Path(__file__).resolve().parents[1];workspace=root.parents[3];output=args.output.resolve()
    for suffix in ('','.build.log','.corpus.zip','-corpus','-bin'):
        if Path(str(output)+suffix).exists():ap.error('preserve evidence; choose a new output stem')
    output.parent.mkdir(parents=True,exist_ok=True);corpus=Path(str(output)+'-corpus');corpus.mkdir()
    sys.path.insert(0,str(workspace/'target/wav/python'));import soundfile as sf
    dr,dr_meta=build_dr_wav(workspace,root);sym,sym_meta=build_symphonia(workspace,root);ffmpeg=shutil.which('ffmpeg');assert ffmpeg
    paths=sorted((root/'src').rglob('*.rs'))+[root/'examples/adpcm_wavl.rs',root/'examples/adpcm_file.rs',root/'examples/adpcm_forward.rs',Path(__file__),root/'tests/adpcm_wavl_fixtures.py',root/'tests/ima_extensible_fixtures.py',root/'tests/ima3_fixtures.py',root/'tests/reference_pcm.py',root/'tests/pcm_fixtures.py',root/'tests/reference_support.py',root/'tests/oracle/dr_wav.c',root/'Cargo.toml']
    before={p.relative_to(root).as_posix():sha(p.read_bytes()) for p in paths}
    command=['cargo','build','--locked','-p','deadlib-audio-decode-wav','--example','adpcm_wavl','--example','adpcm_file','--example','adpcm_forward','--target-dir','target/wav']
    p=subprocess.run(command,cwd=workspace,capture_output=True);Path(str(output)+'.build.log').write_bytes(p.stdout+p.stderr)
    if p.returncode:raise RuntimeError('build failed; see preserved log')
    suffix='.exe' if sys.platform=='win32' else ''
    binaries=Path(str(output)+'-bin');binaries.mkdir()
    for name in ('adpcm_wavl','adpcm_file','adpcm_forward'):
        shutil.copyfile(workspace/('target/wav/debug/examples/'+name+suffix),binaries/(name+suffix))
    binary=binaries/('adpcm_wavl'+suffix);file_binary=binaries/('adpcm_file'+suffix);results=[];manifest={}
    with zipfile.ZipFile(str(output)+'.corpus.zip','x',compression=zipfile.ZIP_DEFLATED,compresslevel=9) as archive:
        def save(name,payload):
            path=corpus/(name+'.wav');path.write_bytes(payload);manifest[path.name]=sha(payload)
            zi=zipfile.ZipInfo(path.name,date_time=(1980,1,1,0,0,0));zi.create_system=3;zi.external_attr=0o600<<16
            archive.writestr(zi,payload,compress_type=zipfile.ZIP_DEFLATED,compresslevel=9)
            return path
        def external(path,truth,combined):
            rows={}
            def record(label,payload,code,stderr=b''):
                rows[label]=dict(exit_code=code,output_bytes=len(payload),output_sha256=sha(payload),spec=compare(payload[:len(truth)],truth,2,code,stderr),combined=compare(payload[:len(combined)],combined,2,code,stderr))
            for label,exe in (('dr_wav',dr),('symphonia',sym)):
                p=subprocess.run([str(exe),str(path)],capture_output=True);record(label,p.stdout,p.returncode,p.stderr)
            p=subprocess.run([ffmpeg,'-v','error','-i',str(path),'-c:a','pcm_s16le','-f','s16le','-'],capture_output=True);record('ffmpeg',p.stdout,p.returncode,p.stderr)
            try:
                data,rate=sf.read(str(path),dtype='int16',always_2d=True);record('libsndfile',data.astype('<i2',copy=False).tobytes(),0);rows['libsndfile']['sample_rate']=rate
            except Exception as exc:record('libsndfile',b'',1,str(exc).encode())
            return rows
        for info,payload,truth,combined,pair,pair_truth,pair_combined in cases():
            path=save(info['name'],payload);ours={};frames=info['frames'];channels=info['channels']
            p=subprocess.run([str(binary),str(path)],capture_output=True);ours['full']=compare(p.stdout,truth,2,p.returncode,p.stderr)
            p=subprocess.run([str(file_binary),str(path)],capture_output=True);ours['reader_full']=compare(p.stdout,truth,2,p.returncode,p.stderr)
            for buffer,width in ((7,1),(127,17)):
                p=subprocess.run([str(binaries/('adpcm_forward'+suffix)),str(path),str(buffer),str(width),str(frames)],capture_output=True)
                ours[f'forward_{buffer}_{width}']=compare(p.stdout,truth,2,p.returncode,p.stderr)
            starts={0,min(1,frames),min(2,frames),frames,max(0,frames-2),min(frames,info['boundaries'][2]),min(frames,info['boundaries'][7]+1)}
            for start in sorted(starts):
                count=min(19,frames-start);p=subprocess.run([str(binary),str(path),str(start),str(count)],capture_output=True)
                ours[f'range_{start}']=compare(p.stdout,truth[start*channels*2:(start+count)*channels*2],2,p.returncode,p.stderr)
                p=subprocess.run([str(file_binary),str(path),str(start),str(count)],capture_output=True)
                ours[f'reader_range_{start}']=compare(p.stdout,truth[start*channels*2:(start+count)*channels*2],2,p.returncode,p.stderr)
            refs=external(path,truth,combined)
            # Positive gates are declared component checks: full legacy blocks,
            # RIFF mono/stereo. No external wavl support/parity is assumed.
            paired=None;gate=[]
            if info['container']=='RIFF' and channels<=2 and info['mode']=='full':
                gate=['ffmpeg'] if info['bits'] in (0,3) else ['dr_wav','libsndfile']
                pair_path=save(info['name']+'-flat-component',pair);paired=external(pair_path,pair_truth,pair_combined)
            passed=all(r['exact'] for r in ours.values()) and all(paired[label]['combined' if info['bits']==3 else 'spec']['exact'] for label in gate)
            results.append(dict(**info,fixture_sha256=sha(payload),truth_sha256=sha(truth),combined_truth_sha256=sha(combined),ours=ours,references=refs,paired_gate=gate,paired=paired,passed=passed))
            if len(results)%36==0:print(f'{len(results)} fixtures; failures={sum(not r["passed"] for r in results)}',flush=True)
    after={p.relative_to(root).as_posix():sha(p.read_bytes()) for p in paths}
    report=dict(timestamp_utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),platform=platform.platform(),rustc=subprocess.check_output(['rustc','-Vv'],text=True),sources=before,inputs_unchanged=before==after,binary_sha256=sha(binary.read_bytes()),reader_binary_sha256=sha(file_binary.read_bytes()),forward_binary_sha256=sha((binaries/('adpcm_forward'+suffix)).read_bytes()),dr_wav=dr_meta,symphonia=sym_meta,libsndfile=sf.__libsndfile_version__,soundfile=sf.__version__,ffmpeg=subprocess.check_output([ffmpeg,'-version'],text=True).splitlines()[0],ffmpeg_binary_sha256=sha(Path(ffmpeg).read_bytes()),corpus_sha256=sha(Path(str(output)+'.corpus.zip').read_bytes()),fixtures=manifest,total=len(results),passed=sum(r['passed'] for r in results),own_checks=sum(len(r['ours']) for r in results),paired_cases=sum(bool(r['paired_gate']) for r in results),cases=results)
    output.write_text(json.dumps(report,indent=2)+'\n');print(f"{report['passed']}/{report['total']} fixtures; {report['own_checks']} borrowed/file/forward/range checks; {report['paired_cases']} paired component gates",flush=True)
    if report['passed']!=report['total'] or before!=after:raise SystemExit(1)


if __name__=='__main__':main()
