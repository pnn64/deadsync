"""Gapless sample counts, PCM and seek coordinates against public libmpg123.

Native audits independently require bit-identical seeks to native linear PCM at
physical-frame boundaries and random positions. External seeks compare both to
their own linear reference and to native PCM with preselected raw PCM limits.
"""
import argparse,hashlib,json,math,struct,subprocess
from pathlib import Path


def chunks(path):
    data=path.read_bytes();assert data[:8]==b'DLMP3FI1';position=8;result=[]
    while position<len(data):
        rate,channels,samples=struct.unpack_from('<III',data,position);position+=12
        assert rate in [8000,11025,12000,16000,22050,24000,32000,44100,48000] and channels in [1,2] and 0<samples<=2_097_152
        count=samples*channels;assert position+count*4<=len(data)
        pcm=struct.unpack_from('<'+'f'*count,data,position);position+=count*4
        result.append(((rate,channels,samples),pcm))
    return result


def flattened(path):
    blocks=chunks(path)
    if not blocks:return None,[]
    format=blocks[0][0][:2];assert all(h[:2]==format for h,_ in blocks)
    return format,[v for _,pcm in blocks for v in pcm]


def difference(a,b):
    assert len(a)==len(b),(len(a),len(b));energy=0;peak=0
    for x,y in zip(a,b):
        assert math.isfinite(x) and math.isfinite(y)
        delta=abs(x-y);energy+=delta*delta;peak=max(peak,delta)
    return {'samples':len(a),'rms':math.sqrt(energy/max(1,len(a))),'max':peak}


def main():
    p=argparse.ArgumentParser()
    for name in ['native','oracle','corpus','output']:p.add_argument('--'+name,type=Path,required=True)
    p.add_argument('--expected-samples',type=int);p.add_argument('--case',action='append')
    p.add_argument('--symphonia',type=Path);p.add_argument('--mad-reference',type=Path);p.add_argument('--mad-oracle',type=Path)
    args=p.parse_args();args.output.mkdir(parents=True,exist_ok=False)
    limits={'rms':2e-6,'max':2e-5};records=[]
    sources=json.loads((args.corpus/'sources.json').read_text()) if (args.corpus/'sources.json').exists() else None
    foundation=json.loads((Path(__file__).parent/'validation/2026-10-27-foundation/reference-framing.json').read_text())
    framing={Path(r['command'][-1]).name:r for r in foundation}
    def run(command,allow_failure=False):
        result=subprocess.run(list(map(str,command)),capture_output=True,text=True)
        record={'command':list(map(str,command)),'exit_code':result.returncode,'stdout':result.stdout,'stderr':result.stderr}
        if not allow_failure:assert result.returncode==0,record
        return record
    def check(d):assert d['rms']<=limits['rms'] and d['max']<=limits['max'],d
    for source in sorted(args.corpus.glob('*.mp3')):
        if args.case and not any(source.name.startswith(c) for c in args.case):continue
        source=source.resolve();native=args.output/(source.stem+'-native.bin');reference=args.output/(source.stem+'-mpg123.bin')
        commands=[run([args.native.resolve(),source,native.resolve(),'gapless','audit']),run([args.oracle.resolve(),source,reference.resolve()])]
        format,a=flattened(native);other,b=flattened(reference);assert format==other
        rate,channels=format;samples=len(a)//channels
        source_samples=sources[source.name]['samples'] if sources else args.expected_samples
        tagged=True
        if not sources:
            proof=framing[source.name];assert hashlib.sha256(source.read_bytes()).hexdigest()==proof['sha256']
            tagged=any(line.startswith('xing\t') for line in proof['native']['stdout'].splitlines())
        expected=source_samples if tagged else sum(int(line.split('\t')[5]) for line in proof['libmad']['stdout'].splitlines() if line.startswith('frame\t'))
        assert expected is not None and samples==expected and len(b)//channels==expected,(source.name,samples,len(b)//channels,expected)
        linear=difference(a,b);check(linear)
        other_decoders={};unsupported=[]
        if args.symphonia:
            target=args.output/(source.stem+'-symphonia.bin')
            command=run([args.symphonia.resolve(),source,target.resolve()],allow_failure=True);commands.append(command)
            if command['exit_code']:
                assert '-free-' in source.name and command['exit_code']==1 and any(reason in command['stderr'] for reason in ['free bit-rate is not supported','core (probe): no suitable format reader found']),command
                unsupported.append({'decoder':'symphonia','reason':command['stderr'].strip()})
            else:
                sym_format,sym=flattened(target);assert sym_format==format
                other_decoders['symphonia']=difference(a,sym);check(other_decoders['symphonia'])
        mad_path=args.mad_reference/(source.stem+'-mad.bin') if args.mad_reference else None
        if args.mad_oracle:
            mad_path=args.output/(source.stem+'-mad-raw.bin');commands.append(run([args.mad_oracle.resolve(),'mad',source,mad_path.resolve()]))
        if mad_path:
            from reference_pcm import frames
            mad=[v for _,pcm in frames(mad_path) for v in pcm]
            start,end,total,raw,physical=map(int,next(line.split('\t')[1:] for line in commands[0]['stdout'].splitlines() if line.startswith('range\t')))
            assert len(mad)==raw*channels and end<=raw
            other_decoders['libmad_cropped_raw']=difference(a,mad[start*channels:end*channels]);check(other_decoders['libmad_cropped_raw'])
        audit=next(line.split('\t')[1:] for line in commands[0]['stdout'].splitlines() if line.startswith('seeks\t'))
        assert audit[-1]=='bit-identical'
        seeks=[]
        for position in sorted({0,1,47,529,575,576,1152,samples//2,samples-1,samples}):
            if position>samples:continue
            count=min(257,samples-position);target=args.output/(source.stem+'-seek-'+str(position)+'.bin')
            command=run([args.oracle.resolve(),source,target.resolve(),position,count])
            seek_format,pcm=flattened(target);assert not pcm or seek_format==format
            assert len(pcm)==count*channels
            ref_delta=difference(pcm,b[position*channels:(position+count)*channels]);check(ref_delta)
            native_delta=difference(pcm,a[position*channels:(position+count)*channels]);check(native_delta)
            seeks.append({'position':position,'count':count,'reference_linear':ref_delta,'native_linear':native_delta,'command':command,'pcm_sha256':hashlib.sha256(target.read_bytes()).hexdigest()})
        record={'file':source.name,'input_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'rate':rate,'channels':channels,'samples':samples,'source_samples':source_samples,'gapless_metadata':tagged,'linear':linear,'other_decoders':other_decoders,'unsupported':unsupported,'native_seek_checks':int(audit[0]),'native_seek_sample_values':int(audit[1]),'seeks':seeks,'commands':commands,
                'pcm_sha256':{p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in [native,reference]}}
        records.append(record);(args.output/'results.json').write_text(json.dumps({'method':__doc__,'limits':limits,'records':records},indent=2)+'\n')
        print(source.name,'samples',samples,'max',linear['max'],'native seeks',audit[0],flush=True)
    binaries=[p for p in [args.native,args.oracle,args.symphonia,args.mad_oracle] if p]
    report={'passed':bool(records),'files':len(records),'tagged_files':sum(r['gapless_metadata'] for r in records),'untagged_files':sum(not r['gapless_metadata'] for r in records),'native_seek_checks':sum(r['native_seek_checks'] for r in records),'reference_seek_checks':sum(len(r['seeks']) for r in records),'binaries':{str(p):hashlib.sha256(p.read_bytes()).hexdigest() for p in binaries}}
    (args.output/'provenance.json').write_text(json.dumps(report,indent=2)+'\n');assert report['passed']


if __name__=='__main__':main()
