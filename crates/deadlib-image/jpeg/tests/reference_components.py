"""Exact many-component gates. No C pixel/color claim or codec dependency."""
import argparse
import hashlib
import itertools
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
from hierarchy_support import structure, check_frame_data
from stream_support import marker_ranges, update_tables, prefix_tables, clear_stale_exp

parser=argparse.ArgumentParser()
for name in ('author','decode','c_author','turbo','ijg','reference','out'):
    parser.add_argument('--'+name.replace('_','-'),required=True,type=Path)
parser.add_argument('--part',choices=('native','c'),required=True)
parser.add_argument('--resume',action='store_true')
parser.add_argument('--reuse-checkpoint',type=Path,help='Reuse completed non-pyramid cases after an observer-only reference update; retain original tool provenance')
parser.add_argument('--fixtures',type=Path)
args=parser.parse_args()
args.out.mkdir(parents=True,exist_ok=True)
work=args.out/'case';work.mkdir(exist_ok=True)
tools=args.out/'tools';tools.mkdir(exist_ok=True)
hashes={}
for name in ('author','decode','c_author','turbo','ijg','reference'):
    source=getattr(args,name);target=tools/(name+source.suffix)
    if not args.resume:shutil.copy2(source,target)
    hashes[name]=hashlib.sha256(target.read_bytes()).hexdigest()
    setattr(args,name,target.resolve())
identity={'tools':hashes,'part':args.part,'driver':hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
checkpoint=args.out/'checkpoint.jsonl'
if args.resume:
    old=[json.loads(line)for line in checkpoint.read_text().splitlines()]
    assert {key:old[0][key]for key in identity}==identity
    identity=old[0]
    done={r['case']:r for r in old[1:]}
elif args.reuse_checkpoint:
    assert args.part=='native'
    old=[json.loads(line)for line in args.reuse_checkpoint.read_text().splitlines()]
    assert old[0]['part']=='native'
    assert {k:v for k,v in old[0]['tools'].items()if k!='reference'}=={k:v for k,v in hashes.items()if k!='reference'}
    done={}
    for row in old[1:]:
        assert row['params'][8]!=2,'mixed-frame observer cases must be rerun'
        row['C_reference_sha256']=old[0]['tools']['reference']if row['exact_jpegxt']else None
        row['validation_driver_sha256']=old[0]['driver']
        done[row['case']]=row
    identity['reused_checkpoint_identity']=old[0]
    with checkpoint.open('w')as f:
        f.write(json.dumps(identity)+'\n')
        for row in done.values():f.write(json.dumps(row)+'\n')
else:
    checkpoint.write_text(json.dumps(identity)+'\n');done={}
records=[];retained=[];coverage=set()
if args.fixtures:args.fixtures.mkdir(parents=True,exist_ok=True)
zigzag=[0,1,8,16,9,2,3,10,17,24,32,25,18,11,4,5,12,19,26,33,40,48,41,34,27,20,13,6,7,14,21,28,35,42,49,56,57,50,43,36,29,22,15,23,30,37,44,51,58,59,52,45,38,31,39,46,53,60,61,54,47,55,62,63]

def sha(data):return hashlib.sha256(data).hexdigest()
def tail(prefix,text):return Path(str(prefix)+text)
def run(command,env=None,strict=True):
    result=subprocess.run([str(s)for s in command],capture_output=True,env=env,timeout=60)
    if strict:assert not result.returncode and not result.stderr and b'warning' not in result.stdout.lower(),(command,result.returncode,result.stderr.decode(errors='replace'))
    return result
def quantizers(data):
    if data[:4]!=b'JCO1':return {}
    count=struct.unpack_from('<I',data,16)[0];position=20;result={}
    for _ in range(count):
        spec=struct.unpack_from('<7I',data,position);position+=28
        result[spec[0]]=struct.unpack_from('<64H',data,position);position+=128+128*spec[5]*spec[6]
    assert position==len(data)
    return result
def quantizer_check(wire,expected,initial):
    bank=initial.copy();fi=-1;specs={};count=0
    for marker,start,end in marker_ranges(wire):
        if marker in (0xdb,0xc4):update_tables(bank,b'\xff\xd8'+wire[start:end]+b'\xff\xd9')
        if marker in (0xc0,0xc1,0xc2,0xc3,0xc5,0xc6,0xc7,0xc9,0xca,0xcb,0xcd,0xce,0xcf):
            fi+=1;payload=wire[start+4:end]
            specs={payload[6+3*i]:payload[8+3*i]for i in range(payload[5])}
        if marker==0xda and expected[fi]:
            header=wire[start+4:end];n=header[0]
            for i in range(n):
                identifier=header[1+2*i];raw=bank[0xdb,specs[identifier]]
                values=tuple(raw[1:])if raw[0]>>4==0 else struct.unpack('>64H',raw[1:])
                natural=[0]*64
                for k,v in enumerate(values):natural[zigzag[k]]=v
                assert tuple(natural)==expected[fi][identifier],('quantizer snapshot',fi,identifier)
                count+=1
    return count
def c_frames(image,frames,expected):
    observation=work/'frames.txt';observation.write_text('')
    result=run([args.reference,'-c','-U',image,work/'pgx'],os.environ|{'DEADLIB_JPEG_FRAME_DUMP':str(observation.resolve())},False)
    reason=None
    if result.returncode or result.stderr or b'warning' in result.stdout.lower():
        reason=result.stderr.decode(errors='replace') or str(result.returncode)
        fixed=clear_stale_exp(image.read_bytes(),frames)
        assert fixed!=image.read_bytes(),reason
        image=work/'c-exp00.jpg';image.write_bytes(fixed);observation.write_text('')
        run([args.reference,'-c','-U',image,work/'pgx'],os.environ|{'DEADLIB_JPEG_FRAME_DUMP':str(observation.resolve())})
    return (*check_frame_data(frames,expected,observation.read_text()),reason)
def save(record):
    record['C_reference_sha256']=hashes['reference']if record['exact_jpegxt']else None
    record['validation_driver_sha256']=identity['driver']
    records.append(record)
    with checkpoint.open('a')as f:f.write(json.dumps(record)+'\n')
    if len(records)%25==0:print('Checked',len(records),args.part,'component cases',flush=True)
def keep(name,paths,expected,keys):
    fresh=set(keys)-coverage
    if not fresh or not args.fixtures:return None
    coverage.update(fresh);files=[]
    for suffix,data in expected:
        dest=args.fixtures/(name+'.'+suffix);dest.write_bytes(data);files.append({'file':dest.name,'sha256':sha(data)})
    streams=[]
    for i,path in enumerate(paths):
        dest=args.fixtures/(name+f'.stream-{i}.jpg');dest.write_bytes(path.read_bytes());streams.append({'file':dest.name,'sha256':sha(dest.read_bytes())})
    row={'case':name,'part':args.part,'streams':streams,'expected':files,'new_coverage':sorted(fresh)};retained.append(row);return row
def native_case(params,name=None):
    n,p,layout,q,arith,fixed,restart,script,mode,abbr,prog=params
    name=name or '-'.join(map(str,params))
    if name in done:
        record=done[name];assert record['params']==list(params);records.append(record)
        if record['fixture']:retained.append(record['fixture']);coverage.update(record['fixture']['new_coverage'])
        return
    (args.out/'current-case.json').write_text(json.dumps({'case':name,'params':params}))
    prefix=work/'native'
    run([args.author,prefix,n,p,layout,q,arith,fixed,restart,script,0,mode,abbr,prog])
    image=tail(prefix,'.jpg');wire=image.read_bytes();bank={};paths=[image]
    if abbr:
        packet=tail(prefix,'.tables.jpg');update_tables(bank,packet.read_bytes(),True);paths=[packet,image]
    header,frames,_,_,_=structure(wire,False)
    expected=[tail(prefix,f'.expected.frame-{i}.{"samples"if f["marker"]&3==3 else"coef"}').read_bytes()for i,f in enumerate(frames)]
    qcount=quantizer_check(wire,[quantizers(data)for data in expected],bank)
    run([args.decode,'decode',work/'decoded','--max-scans',2048,*paths])
    suffixes=[]
    for i,(frame,data)in enumerate(zip(frames,expected)):
        extension='samples'if frame['marker']&3==3 else'coef'
        suffix=f'frame-{i}.{extension}'
        observed=tail(work/'decoded','.image-0.'+(suffix if mode else'coef')).read_bytes()
        assert observed==data,(name,i)
        suffixes.append((suffix,data))
    if mode:
        target=tail(prefix,'.expected.samples').read_bytes()
        assert tail(work/'decoded','.image-0.samples').read_bytes()==target
        suffixes.append(('samples',target))
        if mode==2:assert tail(prefix,'.source.samples').read_bytes()==target
    equivalent=work/'equivalent.jpg';equivalent.write_bytes(prefix_tables(bank,wire))
    if not mode:
        run([args.ijg,equivalent,work/'ijg.coef']);assert(work/'ijg.coef').read_bytes()==expected[0],name
    # JPEG XT rejects fractional sampling in its frame parser. IJG observes
    # those ordinary frames directly; this is not a tolerated C mismatch.
    fractional=any(max(c[1]for c in f['components'])%c[1] or max(c[2]for c in f['components'])%c[2] for f in frames for c in f['components'])
    assert not(mode and fractional),'rare fractional C coverage needs another reference'
    blocks=samples=0;reason=None
    if not fractional:blocks,samples,reason=c_frames(equivalent,frames,expected)
    key=f'm{mode}-p{p}-a{arith}-q{q}-v{prog}-n{n}'
    fixture=keep('native-'+name,paths,suffixes,[key]if n in(4,5,255)and layout in(0,1)and fixed==0 and restart in(0,1)else[])
    save({'case':name,'params':params,'sha256':sha(wire),'bytes':len(wire),'exact_ijg':not mode,'exact_jpegxt':not fractional,
          'quantizer_snapshots':qcount,'C_dct_blocks':blocks,'C_sample_values':samples,'C_EXP00_copy':reason,'fixture':fixture})

def c_case(params):
    n,p,layout,arith,restart,quality,prog,reverse=params;name='c-'+'-'.join(map(str,params))
    if name in done:
        record=done[name];records.append(record)
        if record['fixture']:retained.append(record['fixture']);coverage.update(record['fixture']['new_coverage'])
        return
    (args.out/'current-case.json').write_text(json.dumps({'case':name,'params':params}))
    image=work/'c.jpg';run([args.c_author,image,*params[:5],quality,prog,reverse])
    run([args.decode,'decode',work/'decoded',image]);data=tail(work/'decoded','.image-0.coef').read_bytes()
    run([args.ijg,image,work/'ijg.coef']);assert(work/'ijg.coef').read_bytes()==data,name
    _,frames,_,_,_=structure(image.read_bytes(),False)
    blocks,values,reason=c_frames(image,frames,[data]);assert values==0
    qcount=quantizer_check(image.read_bytes(),[quantizers(data)],{})
    if n<=4:
        run([args.turbo,'decode',work/'turbo',image]);assert tail(work/'turbo','.image-0.coef').read_bytes()==data
    fixture=keep(name,[image],[('frame-0.coef',data)],[f'c-n{n}-p{p}-a{arith}-v{prog}']if n in(4,5,10)and layout==0 and quality==85 and restart==0 and reverse==0 else[])
    save({'case':name,'params':params,'sha256':sha(image.read_bytes()),'bytes':image.stat().st_size,
          'exact_ijg':True,'exact_jpegxt':True,'exact_turbo':n<=4,'C_dct_blocks':blocks,'C_sample_values':0,
          'quantizer_snapshots':qcount,'C_EXP00_copy':reason,'fixture':fixture})

if args.part=='native':
    for n in range(1,256):native_case((n,8 if n%2==0 else 12,n%4,n%4,n%2,0,[0,1,7][n%3],0,0,0,0),'count-'+str(n))
    for n,p,layout,q,arith,script,abbr in itertools.product((5,17,255),(8,12),range(4),range(4),(0,1),(0,2),(0,1)):
        native_case((n,p,layout,q,arith,0,[0,1,7][(n+q+layout)%3],script,0,abbr,0))
    for mode,n,p,layout,q,arith in itertools.product((1,2),(5,17,255),(8,12),(0,2,3),(0,2,3),(0,1)):
        native_case((n,p,layout,q,arith,0,1,0,mode,(n+q)%2,0))
    for n,p,layout,q,arith in itertools.product(range(1,5),(8,12),range(4),(2,3),(0,1)):
        native_case((n,p,layout,q,arith,0,1,0,0,1,1))
else:
    for n,p,layout,arith,restart,reverse in itertools.product(range(1,11),(8,12),(0,2,3),(0,1),(0,1),(0,1)):
        c_case((n,p,layout,arith,restart,85,0,reverse))
    for n,p,layout,arith,restart,reverse in itertools.product(range(1,5),(8,12),(0,2,3),(0,1),(0,1),(0,1)):
        c_case((n,p,layout,arith,restart,85,1,reverse))
report={'part':args.part,'tools':hashes,'driver':identity['driver'],'cases':len(records),
        'reused_checkpoint_identity':identity.get('reused_checkpoint_identity'),
        'C_reference_sha256_used':sorted({r.get('C_reference_sha256',hashes['reference'])for r in records if r['exact_jpegxt']}),
        'exact_ijg':sum(r['exact_ijg']for r in records),'exact_jpegxt':sum(r['exact_jpegxt']for r in records),
        'C_dct_blocks':sum(r['C_dct_blocks']for r in records),'C_sample_values':sum(r['C_sample_values']for r in records),
        'quantizer_snapshots':sum(r['quantizer_snapshots']for r in records),'C_EXP00_copies':sum(bool(r['C_EXP00_copy'])for r in records),
        'scope':'Original full-component syntax and exact coefficients/quantizers; JPEG XT observer-only color bypass, IJG configured for 255 components. No pixel/performance claim.','results':records}
(args.out/'report.json').write_text(json.dumps(report,indent=2)+'\n')
if args.fixtures:(args.fixtures/(args.part+'-manifest.json')).write_text(json.dumps({'tools':hashes,'sessions':retained},indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items()if k!='results'}))
