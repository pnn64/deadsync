"""Native PCM/float API and encoder gate with three independent reference readers."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import subprocess
import sys
import time
import zipfile
from pcm_fixtures import cases
from reference_support import build_dr_wav,build_symphonia


def sha(data):return hashlib.sha256(data).hexdigest()


def compare(actual,expected,width,code=0,stderr=b''):
    first=next((i for i,(a,b) in enumerate(zip(actual,expected)) if a!=b),None)
    if first is None and len(actual)!=len(expected):first=min(len(actual),len(expected))
    return dict(exit_code=code,output_bytes=len(actual),output_sha256=sha(actual),exact=code==0 and actual==expected,first_mismatch=None if first is None else dict(sample=first//width,actual=actual[first//width*width:(first//width+1)*width].hex(),expected=expected[first//width*width:(first//width+1)*width].hex()),stderr=stderr.decode(errors='replace')[:1500])


def main():
    ap=argparse.ArgumentParser();ap.add_argument('--output',type=Path,required=True);ap.add_argument('--limit',type=int);args=ap.parse_args()
    root=Path(__file__).resolve().parents[1];workspace=root.parents[3];stem=args.output.resolve()
    for suffix in ('','.build.log','.corpus.zip','-corpus'):
        if Path(str(stem)+suffix).exists():ap.error('preserve prior evidence; choose a new output')
    stem.parent.mkdir(parents=True,exist_ok=True);corpus=Path(str(stem)+'-corpus');corpus.mkdir()
    sys.path.insert(0,str(workspace/'target/wav/python'));import soundfile as sf
    dr,dr_meta=build_dr_wav(workspace,root,'dr_wav_pcm');sym,sym_meta=build_symphonia(workspace,root,'wav-symphonia-pcm-reference')
    sources=sorted((root/'src').rglob('*.rs'))+[root/'examples/pcm_reference.rs',Path(__file__),root/'tests/pcm_fixtures.py',root/'tests/reference_support.py',root/'tests/oracle/dr_wav_pcm.c',root/'Cargo.toml']
    before={p.relative_to(root).as_posix():sha(p.read_bytes()) for p in sources}
    command=['cargo','build','--locked','-p','deadlib-audio-decode-wav','--example','pcm_reference','--target-dir','target/wav']
    build=subprocess.run(command,cwd=workspace,capture_output=True);Path(str(stem)+'.build.log').write_bytes(build.stdout+build.stderr)
    if build.returncode:raise RuntimeError('native driver build failed; inspect log')
    native=workspace/('target/wav/debug/examples/pcm_reference'+('.exe' if sys.platform=='win32' else ''))
    results=[];manifest={}
    with zipfile.ZipFile(str(stem)+'.corpus.zip','x',compression=zipfile.ZIP_DEFLATED,compresslevel=9) as archive:
        for index,(info,payload,truth) in enumerate(cases()):
            if args.limit is not None and index>=args.limit:break
            name=info['name'];path=corpus/(name+'.wav');path.write_bytes(payload);manifest[name+'.wav']=sha(payload)
            entry=zipfile.ZipInfo(name+'.wav',date_time=(1980,1,1,0,0,0));entry.create_system=3;entry.external_attr=0o600<<16;archive.writestr(entry,payload,compress_type=zipfile.ZIP_DEFLATED,compresslevel=9)
            width=8 if info['kind']=='f64' else 4
            ours={}
            for mode in ('borrowed','reader','forward'):
                if mode=='forward' and info['data_first']:continue
                process=subprocess.run([str(native),str(path),mode],capture_output=True)
                ours[mode]=compare(process.stdout,truth,width,process.returncode,process.stderr)
            start=info['frames']//2;count=min(17,info['frames']-start);stride=info['channels']*width
            for mode in ('borrowed','reader'):
                process=subprocess.run([str(native),str(path),mode,str(start),str(count),'5'],capture_output=True)
                ours[mode+'_range']=compare(process.stdout,truth[start*stride:(start+count)*stride],width,process.returncode,process.stderr)
            encoded={}
            for mode in ('encode','encode-known'):
                process=subprocess.run([str(native),str(path),mode],capture_output=True)
                encoded_path=corpus/(name+'-'+mode+'.wav');encoded_path.write_bytes(process.stdout)
                decoded=subprocess.run([str(native),str(encoded_path),'borrowed'],capture_output=True) if process.returncode==0 else process
                encoded[mode]=dict(encoder_exit_code=process.returncode,encoded_bytes=len(process.stdout),encoded_sha256=sha(process.stdout),**compare(decoded.stdout,truth,width,decoded.returncode,process.stderr+decoded.stderr))
            encoded_equal=encoded['encode']['encoded_sha256']==encoded['encode-known']['encoded_sha256']
            references={}
            for ref,exe in (('dr_wav',dr),('symphonia',sym)):
                process=subprocess.run([str(exe),str(path)],capture_output=True);references[ref]=compare(process.stdout,truth,width,process.returncode,process.stderr)
            try:
                dtype='float64' if info['kind']=='f64' else 'float32' if info['kind']=='f32' else 'int32'
                decoded,rate=sf.read(str(path),dtype=dtype,always_2d=True)
                data=decoded.astype('<f8' if dtype=='float64' else '<f4' if dtype=='float32' else '<i4',copy=False).tobytes()
                references['libsndfile']=compare(data,truth,width)
                references['libsndfile']['sample_rate']=rate;references['libsndfile']['channels']=decoded.shape[1]
            except Exception as error:
                references['libsndfile']=dict(exact=False,error=str(error))
            own_pass=all(v['exact'] for v in ours.values()) and all(v['exact'] and v['encoder_exit_code']==0 for v in encoded.values()) and encoded_equal
            core_pass=not info['core_reference_gate'] or all(v['exact'] for v in references.values())
            results.append(dict(**info,fixture_sha256=sha(payload),truth_sha256=sha(truth),ours=ours,encoded=encoded,encoded_outputs_equal=encoded_equal,references=references,own_pass=own_pass,core_reference_pass=core_pass,passed=own_pass and core_pass))
            if (index+1)%40==0:print(f"{index+1} fixtures checked; own failures={sum(not c['own_pass'] for c in results)}; core reference failures={sum(not c['core_reference_pass'] for c in results)}",flush=True)
    after={p.relative_to(root).as_posix():sha(p.read_bytes()) for p in sources}
    report=dict(timestamp_utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),platform=platform.platform(),python=sys.version,rustc=subprocess.check_output(['rustc','-Vv'],text=True),sources=before,inputs_unchanged=before==after,decoder_sha256=sha(native.read_bytes()),dr_wav=dr_meta,symphonia=sym_meta,soundfile=sf.__version__,libsndfile=sf.__libsndfile_version__,corpus_sha256=sha(Path(str(stem)+'.corpus.zip').read_bytes()),fixtures=manifest,passed=sum(c['passed'] for c in results),own_passed=sum(c['own_pass'] for c in results),core_reference_passed=sum(c['core_reference_pass'] for c in results if c['core_reference_gate']),core_reference_total=sum(c['core_reference_gate'] for c in results),total=len(results),reference_exact={ref:sum(c['references'][ref]['exact'] for c in results) for ref in ('dr_wav','libsndfile','symphonia')},cases=results)
    stem.write_text(json.dumps(report,indent=2)+'\n');print(f"{report['own_passed']}/{report['total']} own native fixture/API/encoder gates; {report['core_reference_passed']}/{report['core_reference_total']} three-reference core gates",flush=True);print(report['reference_exact'],flush=True)
    if report['passed']!=report['total'] or before!=after:raise SystemExit(1)


if __name__=='__main__':main()
