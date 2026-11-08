"""Native wavl I/O, layout-preserving writers and independently flattened oracles."""
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
from wavl_fixtures import cases,chunks
from reference_pcm import compare
from reference_support import build_dr_wav,build_symphonia


def sha(data):return hashlib.sha256(data).hexdigest()


def main():
    ap=argparse.ArgumentParser();ap.add_argument('--output',type=Path,required=True);ap.add_argument('--limit',type=int);args=ap.parse_args()
    root=Path(__file__).resolve().parents[1];workspace=root.parents[3];output=args.output.resolve();output.parent.mkdir(parents=True,exist_ok=True)
    for suffix in ('','.build.log','.corpus.zip','-corpus'):
        if Path(str(output)+suffix).exists():ap.error('preserve earlier evidence; choose a new output')
    corpus=Path(str(output)+'-corpus');corpus.mkdir()
    sys.path.insert(0,str(workspace/'target/wav/python'));import soundfile as sf
    dr,dr_meta=build_dr_wav(workspace,root,'dr_wav_pcm');sym,sym_meta=build_symphonia(workspace,root,'wav-symphonia-pcm-reference');ffmpeg=shutil.which('ffmpeg')
    if not ffmpeg:raise RuntimeError('FFmpeg required')
    source_paths=sorted((root/'src').rglob('*.rs'))+[root/'examples/pcm_reference.rs',Path(__file__),root/'tests/wavl_fixtures.py',root/'tests/pcm_fixtures.py',root/'tests/reference_pcm.py',root/'tests/reference_support.py',root/'tests/oracle/dr_wav_pcm.c',root/'Cargo.toml']
    before={p.relative_to(root).as_posix():sha(p.read_bytes()) for p in source_paths}
    build=subprocess.run(['cargo','build','--locked','-p','deadlib-audio-decode-wav','--example','pcm_reference','--target-dir','target/wav'],cwd=workspace,capture_output=True);Path(str(output)+'.build.log').write_bytes(build.stdout+build.stderr)
    if build.returncode:raise RuntimeError('native driver build failed')
    native=workspace/('target/wav/debug/examples/pcm_reference'+('.exe' if sys.platform=='win32' else ''));results=[];manifest={}
    with zipfile.ZipFile(str(output)+'.corpus.zip','x',compression=zipfile.ZIP_DEFLATED,compresslevel=9) as archive:
        def save(name,payload):
            path=corpus/(name+'.wav');path.write_bytes(payload);manifest[path.name]=sha(payload)
            info=zipfile.ZipInfo(path.name,date_time=(1980,1,1,0,0,0));info.create_system=3;info.external_attr=0o600<<16;archive.writestr(info,payload,compress_type=zipfile.ZIP_DEFLATED,compresslevel=9);return path
        def references(path,info,truth,width):
            rows={}
            for label,exe in (('dr_wav',dr),('symphonia',sym)):
                p=subprocess.run([str(exe),str(path)],capture_output=True);rows[label]=compare(p.stdout,truth,width,p.returncode,p.stderr)
            codec='pcm_f64le' if info['kind']=='f64' else 'pcm_f32le' if info['kind']=='f32' else 'pcm_s32le'
            p=subprocess.run([ffmpeg,'-v','error','-i',str(path),'-c:a',codec,'-f',codec[4:],'-'],capture_output=True);rows['ffmpeg']=compare(p.stdout,truth,width,p.returncode,p.stderr)
            try:
                dtype='float64' if info['kind']=='f64' else 'float32' if info['kind']=='f32' else 'int32'
                decoded,rate=sf.read(str(path),dtype=dtype,always_2d=True);raw=decoded.astype('<f8' if dtype=='float64' else '<f4' if dtype=='float32' else '<i4',copy=False).tobytes();rows['libsndfile']=compare(raw,truth,width);rows['libsndfile'].update(sample_rate=rate,shape=list(decoded.shape))
            except Exception as error:rows['libsndfile']=dict(exact=False,error=str(error))
            return rows
        for index,(info,payload,flat,truth,wire,sequence) in enumerate(cases()):
            if args.limit is not None and index>=args.limit:break
            name=info['name'];path=save(name,payload);flat_path=save(name+'-independent-flat',flat);width=8 if info['kind']=='f64' else 4;stride=width*info['channels'];ours={}
            p=subprocess.run([str(native),str(path),'describe'],capture_output=True)
            try:description=json.loads(p.stdout)
            except Exception:description={}
            wanted=dict(container=info['container'],channels=info['channels'],sample_rate=info['sample_rate'],bits=info['bits'],valid_bits=info['valid_bits'],mask=info['mask'],frames=info['frames'],block_align=info['channels']*info['bits']//8,byte_rate=info['sample_rate']*info['channels']*info['bits']//8)
            description['container']=description.get('container','').upper();ours['describe']=dict(exact=p.returncode==0 and description==wanted,exit_code=p.returncode,descriptor=description,expected=wanted)
            for mode in ('borrowed','reader','forward'):
                p=subprocess.run([str(native),str(path),mode],capture_output=True);ours[mode]=compare(p.stdout,truth,width,p.returncode,p.stderr)
            for mode,start in (('borrowed',2),('reader',info['frames']//2),('forward',2)):
                start=min(start,info['frames']);count=min(17,info['frames']-start);p=subprocess.run([str(native),str(path),mode,str(start),str(count),'5'],capture_output=True);ours[mode+'_range']=compare(p.stdout,truth[start*stride:(start+count)*stride],width,p.returncode,p.stderr)
            encoded={};encoded_flat=None
            for mode in ('encode-wavl','encode-wavl-known','encode','encode-known'):
                p=subprocess.run([str(native),str(path),mode],capture_output=True);encoded_path=save(name+'-'+mode,p.stdout)
                d=subprocess.run([str(native),str(encoded_path),'borrowed'],capture_output=True) if p.returncode==0 else p
                try:
                    pieces=list(chunks(p.stdout,info['container'],len(wire)));audio=next(data for tag,data in pieces if tag==b'LIST' and data.startswith(b'wavl')) if mode.startswith('encode-wavl') else dict(pieces)[b'data']
                    physical_exact=audio==(sequence if mode.startswith('encode-wavl') else wire)
                except Exception:physical_exact=False
                encoded[mode]=dict(encoder_exit_code=p.returncode,encoded_sha256=sha(p.stdout),physical_exact=physical_exact,**compare(d.stdout,truth,width,d.returncode,p.stderr+d.stderr))
                if mode=='encode':encoded_flat=encoded_path
            direct=references(path,info,truth,width);independent_flat=references(flat_path,info,truth,width);written_flat=references(encoded_flat,info,truth,width) if encoded_flat is not None else {}
            pairs_equal=encoded['encode']['encoded_sha256']==encoded['encode-known']['encoded_sha256'] and encoded['encode-wavl']['encoded_sha256']==encoded['encode-wavl-known']['encoded_sha256']
            own_pass=all(row['exact'] for row in ours.values()) and all(row['encoder_exit_code']==0 and row['exact'] and row['physical_exact'] for row in encoded.values()) and pairs_equal
            core_pass=not info['core_reference_gate'] or (all(row['exact'] for row in independent_flat.values()) and all(row['exact'] for row in written_flat.values()))
            results.append(dict(**info,fixture_sha256=sha(payload),independent_flat_sha256=sha(flat),truth_sha256=sha(truth),wire_sha256=sha(wire),ours=ours,encoded=encoded,encoder_pairs_equal=pairs_equal,direct_references=direct,independent_flat_references=independent_flat,encoded_flat_references=written_flat,own_pass=own_pass,core_reference_pass=core_pass,passed=own_pass and core_pass))
            if (index+1)%20==0:print(f'{index+1} wavl cases; failures={sum(not c["passed"] for c in results)}',flush=True)
    after={p.relative_to(root).as_posix():sha(p.read_bytes()) for p in source_paths}
    report=dict(timestamp_utc=time.strftime('%Y-%m-%dT%H:%M:%SZ',time.gmtime()),platform=platform.platform(),rustc=subprocess.check_output(['rustc','-Vv'],text=True),sources=before,inputs_unchanged=before==after,binary_sha256=sha(native.read_bytes()),dr_wav=dr_meta,symphonia=sym_meta,soundfile=sf.__version__,libsndfile=sf.__libsndfile_version__,ffmpeg=subprocess.check_output([ffmpeg,'-version'],text=True).splitlines()[0],ffmpeg_binary_sha256=sha(Path(ffmpeg).read_bytes()),corpus_sha256=sha(Path(str(output)+'.corpus.zip').read_bytes()),fixtures=manifest,passed=sum(c['passed'] for c in results),total=len(results),own_passed=sum(c['own_pass'] for c in results),core_passed=sum(c['core_reference_pass'] for c in results if c['core_reference_gate']),core_total=sum(c['core_reference_gate'] for c in results),cases=results)
    output.write_text(json.dumps(report,indent=2)+'\n');print(f"{report['passed']}/{report['total']} native wavl/layout gates; {report['core_passed']}/{report['core_total']} paired four-reference flat gates",flush=True)
    if report['passed']!=report['total'] or before!=after:raise SystemExit(1)


if __name__=='__main__':main()
