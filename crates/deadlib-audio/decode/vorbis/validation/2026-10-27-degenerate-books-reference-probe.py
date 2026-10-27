from pathlib import Path
import hashlib,json,struct,subprocess,collections,math
repo=Path.cwd();root=repo/'target/deadlib-audio-reference';run=root/'degenerate-books-20261027';inputs=root/'degenerate-books-validation-20261027/configuration-inputs';seeds=root/'degenerate-books-seeds-20261027'
exe=root/'vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe';sha=lambda b:hashlib.sha256(b).hexdigest()
def packed(packets):return b''.join(struct.pack('<Q',len(p))+p for p in packets)
def request(raw):
 at=0
 def u64():
  nonlocal at
  n=int.from_bytes(raw[at:at+8],'little');at+=8;return n
 def packet():
  nonlocal at
  n=u64();b=raw[at:at+n];at+=n;return b
 headers=[packet() for _ in range(3)];cases=[]
 for _ in range(u64()):cases.append([packet() for _ in range(u64())])
 assert at==len(raw);return headers,cases

def oracle(mode,raw,suffix='bin'):
 p=run/('independent-input.'+suffix);p.write_bytes(raw)
 return subprocess.check_output([str(exe),mode,str(p)])
def normalized(raw):
 assert len(raw)%4==0
 return b''.join(struct.pack('<I',0x7fc00000 if (n&0x7f800000)==0x7f800000 and n&0x7fffff else n) for (n,) in struct.iter_unpack('<I',raw))
paths=sorted(p for p in inputs.glob('*.bin') if p.name.startswith(('empty-','scalar-')))
groups=collections.defaultdict(list)
for p in paths:groups[p.stem.split('-candidate-')[0]].append(p)
assert len(groups)==312,len(groups)
canonical=''.join(p.name+'\t'+sha(p.read_bytes())+'\n' for p in paths)
selected=sorted(set(p for g in groups.values() for p in [g[0],g[-1]]))
selected+=sorted((run/'golden-inputs').glob('*.bin'))+[inputs/'nonfinite-reference.bin']
checks=[];fresh=0
for p in selected:
 headers,cases=request(p.read_bytes());records=[]
 indices=range(len(cases)) if p.parent.name=='golden-inputs' or p.stem=='nonfinite-reference' else sorted(set([0,len(cases)//2,len(cases)-1]))
 for i in indices:
  packets=cases[i];a=oracle('pcm-cases',packed(headers)+struct.pack('<QQ',1,len(packets))+packed(packets));b=oracle('pcm',packed(headers+packets))
  assert a[:4]==b[:4]
  # Metadata includes byte decisions and integer counts, so compare its raw
  # bytes; NaN payload normalization is used only on full container PCM below.
  assert a[12:]==b[4:],(p.name,i)
  records.append({'case':i,'fresh_pcm_sha256':sha(b),'protocol_bytes':len(b),'byte_identical':True});fresh+=1
 checks.append({'path':p.relative_to(repo).as_posix(),'sha256':sha(p.read_bytes()),'batch_cases':len(cases),'checks':records})
golden=b'DVCBOOK1'+struct.pack('<I',11);ogg_checks=[]
for i in range(11):
 p=run/'golden-inputs'/f'book-golden-{i}.bin';headers,cases=request(p.read_bytes());out=oracle('pcm-cases',p.read_bytes());golden+=struct.pack('<Q',len(out))+out
 channels=int.from_bytes(out[:4],'little');at=12;planes=[bytearray() for _ in range(channels)];raw_frames=0
 for j,case in enumerate(cases):
  for _ in case:
   kind=out[at];at+=1;assert kind==ord('P');n,frames=struct.unpack_from('<II',out,at);at+=8
   if j==0:raw_frames+=frames
   for plane in planes:
    if j==0:plane.extend(out[at:at+frames*4])
    at+=frames*4
 assert at==len(out) and raw_frames==368
 seed=seeds/'vorbis_stream'/f'synthetic-book-golden-{i}-case-0-control-48.bin';data=seed.read_bytes()[1:]
 ogg=oracle('oggpcm',data,'ogg');assert struct.unpack_from('<IIIIQ',ogg,0)==(1,71,channels,48000,368)
 got=[bytearray() for _ in range(channels)];at=24;emitted=0
 while at<len(ogg):
  link,frames=struct.unpack_from('<II',ogg,at);at+=8;assert link==0;emitted+=frames
  for plane in got:plane.extend(ogg[at:at+frames*4]);at+=frames*4
 assert at==len(ogg) and emitted==368 and all(normalized(a)==normalized(b) for a,b in zip(got,planes))
 targets=[0,1,79,80,81,207,208,287,288,367,368]
 q=run/'independent-seeks.bin';q.write_bytes(struct.pack('<Q',len(targets))+b''.join(struct.pack('<QQ',t,368-t) for t in targets))
 media=run/'independent-input.ogg';media.write_bytes(data)
 result=subprocess.run([str(exe),'oggseek',str(media),str(q)],capture_output=True);assert result.returncode==0,(i,result.stderr)
 seek=result.stdout;assert struct.unpack_from('<IIIIQ',seek,0)==(1,71,channels,48000,368);at=24;seek_records=[]
 for target in targets:
  position=struct.unpack_from('<Q',seek,at)[0];at+=8;got=[bytearray() for _ in range(channels)];emitted=0
  while True:
   frames=struct.unpack_from('<I',seek,at)[0];at+=4
   if frames==0:break
   link=struct.unpack_from('<I',seek,at)[0];at+=4;assert link==0;emitted+=frames
   for plane in got:plane.extend(seek[at:at+frames*4]);at+=frames*4
  match=all(normalized(a)==normalized(b[target*4:]) for a,b in zip(got,planes))
  assert position==target and emitted==368-target and match,(i,target,position,emitted,match)
  seek_records.append({'target':target,'position':position,'frames':emitted,'raw_pcm_identical_except_nan_payload':match})
 assert at==len(seek)
 ogg_checks.append({'golden':i,'channels':channels,'input_sha256':sha(data),'frames':368,'raw_pcm_identical_except_nan_payload':True,'oggseek_exit_code':0,'seeks':seek_records})
fixture=repo/'crates/deadlib-audio/decode/vorbis/tests/fixtures/degenerate-books-v1.libvorbis-pcm';assert golden==fixture.read_bytes()
# Preserve the original rejection probes and independently verify C acceptance.
# Expanded channel/coupling geometry can change headers, so report exact final
# matches only where present rather than claiming every matrix request is equal.
by_headers=collections.defaultdict(list)
for p in paths:
 headers,_=request(p.read_bytes());by_headers[sha(packed(headers))].append(p.name)
baselines=[]
for p in sorted((run/'baseline-inputs').glob('*.bin')):
 headers,cases=request(p.read_bytes());out=oracle('pcm-cases',p.read_bytes())
 baselines.append({'path':p.relative_to(repo).as_posix(),'sha256':sha(p.read_bytes()),'reference_response_sha256':sha(out),'reference_accepted':True,'cases':len(cases),'final_requests_with_identical_headers':by_headers[sha(packed(headers))]})
summary={'scope':'Independent fresh-process case reset protocol, exact retained C golden capture, C Ogg counts/PCM and C seeks. Finite PCM/infinity signs exact within this C selfcheck; NaN payloads normalized only for container PCM. Native comparisons use original tolerances and exact decisions/counts/positions.','reference_binary_sha256':sha(exe.read_bytes()),'matrix_setup_combinations':312,'matrix_request_files':len(paths),'matrix_request_bytes':sum(p.stat().st_size for p in paths),'matrix_input_digest_sha256':sha(canonical.encode()),'digest_definition':'UTF-8 sorted request filename, tab, lowercase raw request SHA-256, LF, for empty-/scalar- files.','fresh_process_cases':fresh,'selected_requests':checks,'golden':{'bytes':len(golden),'sha256':sha(golden),'oracle_byte_identical':True},'baseline_headers':baselines,'container_reference_checks':ogg_checks}
(run/'independent-reference.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf8',newline='\n')
print('Checked',fresh,'fresh-process cases;',len(paths),'matrix requests;',len(ogg_checks),'C Ogg streams;',sum(len(x['seeks']) for x in ogg_checks),'C seeks.')
