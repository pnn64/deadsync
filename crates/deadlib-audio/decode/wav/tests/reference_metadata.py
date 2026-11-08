"""Independent RIFF/BWF fixtures and a pinned dr_wav metadata differential gate."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import subprocess
import sys
from reference_support import build_dr_wav


def sha(b):
    return hashlib.sha256(b).hexdigest()


def ck(tag,data):
    return tag+struct.pack('<I',len(data))+data+b'\0'*(len(data)&1)


def main():
    ap=argparse.ArgumentParser()
    ap.add_argument('--output',type=Path,required=True)
    args=ap.parse_args()
    root=Path(__file__).resolve().parents[1]
    workspace=root.parents[3]
    oracle,provenance=build_dr_wav(workspace,root,'dr_wav_metadata')
    subprocess.run(['cargo','build','--locked','-p','deadlib-audio-decode-wav','--example','metadata_fields','--target-dir','target/wav'],cwd=workspace,capture_output=True,check=True)
    binary=workspace/('target/wav/debug/examples/metadata_fields'+('.exe' if sys.platform=='win32' else ''))
    corpus=workspace/'target/wav/metadata-corpus'
    corpus.mkdir(parents=True,exist_ok=True)
    results=[]
    for version in (0,1,2):
        bext=bytearray(602)
        bext[:5]=b'audio'
        bext[256:262]=b'studio'
        bext[288:291]=b'ref'
        bext[320:338]=b'2026-10-2812:00:00'
        struct.pack_into('<QH',bext,338,8294400000,version)
        if version>=1:
            bext[348:412]=bytes(range(64))
        if version>=2:
            struct.pack_into('<5h',bext,412,-2300,1200,-100,-1800,-2100)
        bext.extend(b'A=PCM,F=192000,W=16,M=stereo\r\n')
        cue=struct.pack('<I6I6I',2,7,0,int.from_bytes(b'data','little'),0,0,0,8,1,int.from_bytes(b'data','little'),0,0,1)
        smpl=struct.pack('<9I12I',0x01000013,17,5208,60,0x80000000,25,0xff010203,2,3,
            7,0,0,1,0x40000000,0,8,1,0,1,0,3)+b'\x01\x02\x03'
        info=b'INFO'+ck(b'INAM',b'caf\xe9\0')+ck(b'IART',b'artist\0')
        adtl=b'adtl'+ck(b'labl',struct.pack('<I',7)+b'label\0')+ck(b'note',struct.pack('<I',8)+b'note\0')+ck(b'ltxt',struct.pack('<II4s4H',7,2,b'rgn ',44,9,1,1252)+b'region\0')
        truth=sorted([
            f"bext {version} 8294400000 617564696f 73747564696f 726566 323032362d31302d3238 31323a30303a3030 "+(bytes(range(64)).hex() if version>=1 else '-')+' '+('-2300,1200,-100,-1800,-2100' if version>=2 else '-')+' '+b'A=PCM,F=192000,W=16,M=stereo\r\n'.hex(),
            'cue 7 0 64617461 0 0 0','cue 8 1 64617461 0 0 1',
            'smpl 16777235 17 5208 60 2147483648 25 4278256131 2 010203',
            'loop 7 0 0 1 1073741824 0','loop 8 1 0 1 0 3',
            'info INAM 636166e9','info IART 617274697374','labl 7 6c6162656c','note 8 6e6f7465',
            'ltxt 7 2 72676e20 44 9 1 1252 726567696f6e',
        ])
        metadata=ck(b'bext',bext)+ck(b'cue ',cue)+ck(b'smpl',smpl)+ck(b'LIST',info)+ck(b'LIST',adtl)
        fmt=ck(b'fmt ',struct.pack('<HHIIHH',1,2,192000,768000,4,16))
        audio=struct.pack('<4h',0,-1,32767,-32768)
        for rf64 in (False,True):
            for trailing in (False,True):
                data=b'data'+struct.pack('<I',0xffffffff if rf64 else len(audio))+audio
                body=b'WAVE'+fmt+(data+metadata if trailing else metadata+data)
                if rf64:
                    ds64=ck(b'ds64',struct.pack('<QQQI',len(body)+36,len(audio),2,0))
                    payload=b'RF64'+b'\xff'*4+b'WAVE'+ds64+body[4:]
                else:
                    payload=b'RIFF'+struct.pack('<I',len(body))+body
                name=f'bext{version}-'+('rf64' if rf64 else 'riff')+('-trailing' if trailing else '-leading')
                path=corpus/f'{name}.wav'
                path.write_bytes(payload)
                actual=subprocess.run([str(binary),str(path)],capture_output=True,check=True).stdout
                expected=subprocess.run([str(oracle),str(path)],capture_output=True,check=True).stdout
                a=sorted(actual.decode().splitlines())
                b=sorted(expected.decode().splitlines())
                results.append(dict(name=name,fixture_sha256=sha(payload),passed=a==truth,reference_exact=a==b,
                                    actual=a,reference=b,independent_field_truth=truth))
    sources=sorted((root/'src').glob('*.rs'))+sorted((root/'src/metadata').glob('*.rs'))+[root/'examples/metadata_fields.rs',Path(__file__),root/'tests/reference_support.py']
    report=dict(oracle=provenance,binary_sha256=sha(binary.read_bytes()),sources={p.relative_to(root).as_posix():sha(p.read_bytes()) for p in sources},
                passed=sum(r['passed'] for r in results),reference_exact=sum(r['reference_exact'] for r in results),total=len(results),cases=results)
    args.output.parent.mkdir(parents=True,exist_ok=True)
    args.output.write_text(json.dumps(report,indent=2)+'\n')
    print(f"{report['passed']}/{report['total']} metadata fixture field checks passed; {report['reference_exact']} exact dr_wav comparisons")
    if report['passed']!=report['total']:
        raise SystemExit(1)


if __name__=='__main__':
    main()
