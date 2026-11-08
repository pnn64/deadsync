"""Decode independently encoded WAV files through four reference implementations."""
import argparse
import hashlib
import json
import math
from pathlib import Path
import random
import shutil
import struct
import subprocess
import sys
import wave
from reference_support import build_dr_wav, build_symphonia


def run(command):
    return subprocess.run([str(v) for v in command],capture_output=True,check=True)


def sha(b):
    return hashlib.sha256(b).hexdigest()


def main():
    ap=argparse.ArgumentParser()
    ap.add_argument('--output',type=Path,required=True)
    args=ap.parse_args()
    root=Path(__file__).resolve().parents[1]
    workspace=root.parents[3]
    sys.path.insert(0,str(workspace/'target/wav/python'))
    import soundfile as sf
    dr,dr_meta=build_dr_wav(workspace,root)
    sym,sym_meta=build_symphonia(workspace,root)
    ffmpeg=shutil.which('ffmpeg')
    if not ffmpeg:
        raise RuntimeError('ffmpeg is required')
    run(['cargo','build','--locked','-p','deadlib-audio-decode-wav','--examples','--target-dir','target/wav'])
    suffix='.exe' if sys.platform=='win32' else ''
    encoder=workspace/f'target/wav/debug/examples/encode_adpcm{suffix}'
    decoder=workspace/f'target/wav/debug/examples/adpcm{suffix}'
    corpus=workspace/'target/wav/encoded-adpcm'
    corpus.mkdir(parents=True,exist_ok=True)
    results=[]
    for codec in ('ms','ima'):
        for channels in (1,2):
            for per_channel in (8 if codec=='ima' else 9,32,256):
                for length in (1,17,1025):
                    for signal in ('constant','sine','noise','impulse'):
                        rng=random.Random(0x574156)
                        samples=[]
                        for i in range(length):
                            for c in range(channels):
                                value=12345 if signal=='constant' else int(30000*math.sin((i+c*7)*.073)) if signal=='sine' else rng.randint(-32768,32767) if signal=='noise' else (32767 if i%257==c else 0)
                                samples.append(value)
                        source=corpus/'source.wav'
                        with wave.open(str(source),'wb') as sink:
                            sink.setparams((channels,2,44100,0,'NONE','not compressed'))
                            sink.writeframes(struct.pack('<'+'h'*len(samples),*samples))
                        payload=run([encoder,source,codec,per_channel*channels]).stdout
                        name=f'{codec}-{channels}-{per_channel*channels}-{length}-{signal}'
                        path=corpus/f'{name}.wav'
                        path.write_bytes(payload)
                        decoded=run([decoder,path]).stdout
                        exact_length=len(decoded)==len(samples)*2
                        compared={}
                        for ref,command in [('dr_wav',[dr,path]),('ffmpeg',[ffmpeg,'-v','error','-i',path,'-f','s16le','-']),('symphonia',[sym,path])]:
                            process=subprocess.run([str(c) for c in command],capture_output=True)
                            expected=process.stdout
                            trimmed=expected[:len(decoded)]
                            compared[ref]=dict(exit_code=process.returncode,full_samples=len(expected)//2,
                                full_sha256=sha(expected),fact_trimmed_sha256=sha(trimmed),fact_trimmed_exact=process.returncode==0 and trimmed==decoded,
                                first_mismatch=next((i//2 for i,(a,b) in enumerate(zip(decoded,trimmed)) if a!=b),None),
                                stderr=process.stderr.decode(errors='replace')[:1000])
                        try:
                            expected=sf.read(str(path),dtype='int16',always_2d=True)[0].astype('<i2').tobytes()
                            trimmed=expected[:len(decoded)]
                            compared['libsndfile']=dict(full_samples=len(expected)//2,full_sha256=sha(expected),fact_trimmed_sha256=sha(trimmed),fact_trimmed_exact=trimmed==decoded,
                                first_mismatch=next((i//2 for i,(a,b) in enumerate(zip(decoded,trimmed)) if a!=b),None))
                        except sf.LibsndfileError as error:
                            log=sf._ffi.new('char[]',4096)
                            sf._snd.sf_command(sf._ffi.NULL,0x1001,log,4096)
                            compared['libsndfile']=dict(fact_trimmed_exact=False,error=str(error),log=sf._ffi.string(log).decode(errors='replace'))
                        values=struct.unpack('<'+'h'*(len(decoded)//2),decoded)
                        error=sum((a-b)**2 for a,b in zip(samples,values))
                        power=sum(a*a for a in samples)
                        gate=('dr_wav','libsndfile') if codec=='ima' else ('ffmpeg',)
                        passed=exact_length and all(compared[r]['fact_trimmed_exact'] for r in gate)
                        results.append(dict(name=name,fixture_sha256=sha(payload),encoded_bytes=len(payload),source_samples=len(samples),decoded_samples=len(values),
                            decoded_sha256=sha(decoded),references=compared,spec_arithmetic_gate=gate,passed=passed,
                            rmse=math.sqrt(error/len(samples)),snr_db=10*math.log10(power/error) if error and power else None,
                            max_abs_error=max(abs(a-b) for a,b in zip(samples,values)),exact_source=decoded==struct.pack('<'+'h'*len(samples),*samples)))
    paths=sorted((root/'src').rglob('*.rs'))+[root/'Cargo.toml',root/'examples/encode_adpcm.rs',root/'examples/adpcm.rs',Path(__file__),root/'tests/reference_support.py']
    report=dict(rustc=run(['rustc','-V']).stdout.decode().strip(),ffmpeg=run([ffmpeg,'-version']).stdout.decode().splitlines()[0],
        libsndfile=sf.__libsndfile_version__,soundfile=sf.__version__,dr_wav=dr_meta,symphonia=sym_meta,
        encoder_sha256=sha(encoder.read_bytes()),decoder_sha256=sha(decoder.read_bytes()),sources={p.relative_to(root).as_posix():sha(p.read_bytes()) for p in paths},
        passed=sum(c['passed'] for c in results),total=len(results),cases=results)
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(report,indent=2)+'\n')
    print(f"{report['passed']}/{report['total']} encoder/reference cases pass specified arithmetic and exact fact length")
    for ref in ('dr_wav','libsndfile','ffmpeg','symphonia'):
        print(ref,sum(c['references'][ref]['fact_trimmed_exact'] for c in results),'/',len(results),'exact after fact trimming')
    if report['passed']!=report['total']:
        raise SystemExit(1)


if __name__=='__main__':
    main()
