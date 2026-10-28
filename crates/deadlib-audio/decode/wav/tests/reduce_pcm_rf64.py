"""Reproduce the observed libsndfile RF64 odd-JUNK parsing distinction."""
import argparse
import hashlib
import json
from pathlib import Path
import struct
import sys
import zipfile


def main():
    ap=argparse.ArgumentParser();ap.add_argument('--output',type=Path,required=True);args=ap.parse_args()
    root=Path(__file__).resolve().parents[1];workspace=root.parents[3];output=args.output.resolve()
    if output.exists() or Path(str(output)+'.corpus.zip').exists():ap.error('choose a new output')
    sys.path.insert(0,str(workspace/'target/wav/python'));import soundfile as sf
    output.parent.mkdir(parents=True,exist_ok=True);directory=Path(str(output)+'-files');directory.mkdir()
    def chunk(tag,data,pad=0):return tag+struct.pack('<I',len(data))+data+bytes([pad])*(len(data)&1)
    fmt=chunk(b'fmt ',struct.pack('<HHIIHH',1,2,48000,192000,4,16));audio=struct.pack('<8h',0,-1,32767,-32768,1,2,3,4);data=b'data'+b'\xff'*4+audio
    variants={'bare':fmt,'odd-junk-before':chunk(b'JUNK',b'abc')+fmt,'odd-junk-after':fmt+chunk(b'JUNK',b'abc'),'even-junk-before':chunk(b'JUNK',b'abcd')+fmt,'odd-junk-nonzero-pad':fmt+chunk(b'JUNK',b'abc',0xa5),'info':fmt+chunk(b'LIST',b'INFO'+chunk(b'INAM',b'cafe\0'))}
    cases=[]
    with zipfile.ZipFile(str(output)+'.corpus.zip','x',compression=zipfile.ZIP_DEFLATED,compresslevel=9) as archive:
        for name,leading in variants.items():
            body=b'WAVE'+leading+data;payload=b'RF64'+b'\xff'*4+b'WAVE'+chunk(b'ds64',struct.pack('<QQQI',len(body)+36,len(audio),4,0))+body[4:]
            path=directory/(name+'.wav');path.write_bytes(payload);info=zipfile.ZipInfo(path.name,date_time=(1980,1,1,0,0,0));info.create_system=3;info.external_attr=0o600<<16;archive.writestr(info,payload,compress_type=zipfile.ZIP_DEFLATED,compresslevel=9)
            row=dict(name=name,fixture_sha256=hashlib.sha256(payload).hexdigest(),zero_padding='nonzero' not in name)
            try:
                decoded,rate=sf.read(str(path),dtype='int16',always_2d=True);raw=decoded.astype('<i2',copy=False).tobytes();row.update(accepted=True,exact=raw==audio,output_sha256=hashlib.sha256(raw).hexdigest(),rate=rate)
            except Exception as error:row.update(accepted=False,exact=False,error=str(error))
            cases.append(row)
    report=dict(libsndfile=sf.__libsndfile_version__,soundfile=sf.__version__,source_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),corpus_sha256=hashlib.sha256(Path(str(output)+'.corpus.zip').read_bytes()).hexdigest(),cases=cases)
    output.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({c['name']:c['accepted'] for c in cases}))


if __name__=='__main__':main()
