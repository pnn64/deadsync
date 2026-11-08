from pathlib import Path
import hashlib,json,struct,subprocess,collections
repo=Path.cwd();root=repo/'target/deadlib-audio-reference';run=root/'residue-geometry-20261027';inputs=root/'residue-geometry-validation-20261027/configuration-inputs'
exe=root/'vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe'
sha=lambda b:hashlib.sha256(b).hexdigest()
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
 assert at==len(raw)
 return headers,cases
def oracle(mode,raw,suffix='bin'):
 p=run/('independent-input.'+suffix);p.write_bytes(raw)
 return subprocess.check_output([str(exe),mode,str(p)])
paths=sorted(p for p in inputs.glob('*.bin') if p.name.startswith(('geometry-ch-','unaligned-ch-')))
groups=collections.defaultdict(list);canonical=[];total_bytes=0
for p in paths:
 raw=p.read_bytes();h=sha(raw);canonical.append(p.name+'\t'+h+'\n');total_bytes+=len(raw)
 groups[p.stem.split('-seed-')[0]].append(p)
assert len(groups)==150,len(groups)
initial=run/'inputs'
for p in paths:
 if p.name.startswith('geometry-ch-'):assert sha(p.read_bytes())==sha((initial/p.name).read_bytes()),p.name
selected=sorted(set(p for group in groups.values() for p in [group[0],group[-1]]))
selected+=sorted((run/'golden-inputs').glob('*.bin'))
assert len(selected)==304
checks=[];fresh_count=0
for p in selected:
 headers,cases=request(p.read_bytes());records=[]
 for i in sorted(set([0,len(cases)//2,len(cases)-1])):
  packets=cases[i];wire=packed(headers)+struct.pack('<QQ',1,len(packets))+packed(packets)
  a=oracle('pcm-cases',wire);b=oracle('pcm',packed(headers+packets));assert a[:4]==b[:4] and a[12:]==b[4:],(p.name,i)
  records.append({'case':i,'fresh_pcm_sha256':sha(b),'protocol_bytes':len(b)});fresh_count+=1
 checks.append({'path':p.relative_to(repo).as_posix(),'sha256':sha(p.read_bytes()),'batch_cases':len(cases),'checks':records})
# Exact retained golden capture, separately from numerical native comparison.
golden=b'DVRGEO01'+struct.pack('<I',4);ogg_checks=[]
for i in range(4):
 p=run/'golden-inputs'/f'geometry-golden-{i}.bin';headers,cases=request(p.read_bytes());assert len(cases)==1
 out=oracle('pcm-cases',p.read_bytes());golden+=struct.pack('<Q',len(out))+out
 channels=int.from_bytes(out[:4],'little');at=12;planes=[bytearray() for _ in range(channels)];raw_frames=0
 for _ in cases[0]:
  kind=out[at];at+=1;assert kind==ord('P');n,frames=struct.unpack_from('<II',out,at);at+=8;raw_frames+=frames
  for plane in planes:plane.extend(out[at:at+frames*4]);at+=frames*4
 assert at==len(out) and raw_frames==368
 seed=root/'residue-geometry-seeds-20261027/vorbis_stream'/f'synthetic-geometry-golden-{i}-control-0.bin'
 data=seed.read_bytes()[1:];ogg=oracle('oggpcm',data,'ogg');links,serial,ch,rate,total=struct.unpack_from('<IIIIQ',ogg,0)
 assert (links,serial,ch,rate,total)==(1,71,channels,48000,368)
 got=[bytearray() for _ in range(channels)];at=24;emitted=0
 while at<len(ogg):
  link,frames=struct.unpack_from('<II',ogg,at);at+=8;assert link==0;emitted+=frames
  for plane in got:plane.extend(ogg[at:at+frames*4]);at+=frames*4
 assert at==len(ogg) and emitted==368 and got==planes
 # Verify libvorbisfile's seeks against its raw packet PCM on these probes.
 targets=[0,1,79,80,81,207,208,287,288,367,368]
 requests=struct.pack('<Q',len(targets))+b''.join(struct.pack('<QQ',t,368-t) for t in targets)
 q=run/'independent-seeks.bin';q.write_bytes(requests);media=run/'independent-input.ogg';media.write_bytes(data)
 result=subprocess.run([str(exe),'oggseek',str(media),str(q)],capture_output=True)
 seek_records=[]
 if result.returncode==0:
  seek=result.stdout;assert struct.unpack_from('<IIIIQ',seek,0)==(1,71,channels,48000,368);at=24
  for target in targets:
   position=struct.unpack_from('<Q',seek,at)[0];at+=8;got=[bytearray() for _ in range(channels)];emitted=0
   while True:
    frames=struct.unpack_from('<I',seek,at)[0];at+=4
    if frames==0:break
    link=struct.unpack_from('<I',seek,at)[0];at+=4;assert link==0;emitted+=frames
    for plane in got:plane.extend(seek[at:at+frames*4]);at+=frames*4
   seek_records.append({'target':target,'position':position,'frames':emitted,'raw_pcm_identical':all(a==b[target*4:] for a,b in zip(got,planes))})
  assert at==len(seek)
 ogg_checks.append({'golden':i,'channels':channels,'input_sha256':sha(data),'frames':368,'raw_pcm_byte_identical':True,'oggseek_exit_code':result.returncode,'oggseek_stderr':result.stderr.decode(errors='replace'),'seeks':seek_records})
fixture=repo/'crates/deadlib-audio/decode/vorbis/tests/fixtures/residue-layout-v1.libvorbis-pcm';assert golden==fixture.read_bytes()
assert sha(golden)=='92b5e01f6a1df2496d8e67fb77ac6b9b9f0192750a3b85897db17ae728ce3fda'
assert sha((run/'unaligned-inputs/unaligned-residue-two.bin').read_bytes())==sha((run/'golden-inputs/geometry-golden-0.bin').read_bytes())
summary={'scope':'Independent reset-case protocol, exact C golden capture, libvorbisfile framing/count/PCM/seek checks. These checks are separate from native PCM tolerances.','reference_binary_sha256':sha(exe.read_bytes()),'matrix_setup_combinations':150,'matrix_request_files':len(paths),'matrix_request_bytes':total_bytes,'matrix_input_digest_sha256':sha(''.join(canonical).encode()),'digest_definition':'UTF-8: sorted request filename, tab, lower-case SHA-256 of raw request, LF, for geometry-ch-/unaligned-ch- files.','aligned_requests_identical_to_initial_snapshot':True,'fresh_process_cases':fresh_count,'selected_requests':checks,'golden':{'bytes':len(golden),'sha256':sha(golden),'oracle_byte_identical':True,'baseline_failure_input_identical':True},'container_reference_checks':ogg_checks}
(run/'independent-reference.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf8',newline='\n')
print('Checked',fresh_count,'fresh-process cases;',len(paths),'matrix requests;',len(ogg_checks),'independent Ogg count/PCM probes.')
print('C seek outcomes:',[(x['golden'],x['oggseek_exit_code'],all(s['position']==s['target'] and s['frames']==368-s['target'] and s['raw_pcm_identical'] for s in x['seeks'])) for x in ogg_checks])
