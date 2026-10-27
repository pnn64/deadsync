from pathlib import Path
import hashlib,json,struct,subprocess,csv
repo=Path.cwd();root=repo/'target/deadlib-audio-reference';run=root/'codebook-extents-20261027';gates=root/'codebook-extents-validation-verified-20261027'
exe=root/'vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe';old=run/'oracle-before.exe';sha=lambda b:hashlib.sha256(b).hexdigest()
def packed(packets):return b''.join(struct.pack('<Q',len(p))+p for p in packets)
def unpack(raw,count=False):
 at=0
 def u64():
  nonlocal at
  n=int.from_bytes(raw[at:at+8],'little');at+=8;return n
 def packet():
  nonlocal at
  n=u64();b=raw[at:at+n];at+=n;return b
 if count:packets=[packet() for _ in range(u64())]
 else:packets=[packet() for _ in range(3)]
 return packets,at

def oracle(mode,raw,binary=exe):
 p=run/'independent-input.bin';p.write_bytes(raw)
 return subprocess.run([str(binary),mode,str(p)],capture_output=True)
paths=sorted((gates/'book-inputs').glob('book-extents-*.bin'),key=lambda p:int(p.stem.rsplit('-',1)[1]));assert len(paths)==30
books=[];capture=b'DVBEXT01'+struct.pack('<Q',475);requests=[]
for p in paths:
 raw=p.read_bytes();batch,at=unpack(raw,True);assert at==len(raw);books+=batch
 r=oracle('bookheaders',raw);assert r.returncode==0 and int.from_bytes(r.stdout[:8],'little')==len(batch);capture+=r.stdout[8:]
 requests.append({'path':p.relative_to(repo).as_posix(),'sha256':sha(raw),'bytes':len(raw),'declarations':len(batch)})
fixture=repo/'crates/deadlib-audio/decode/vorbis/tests/fixtures/codebook-extents-v1.libvorbis-headers';assert capture==fixture.read_bytes() and len(books)==475
rows=[]
for i,book in enumerate(books):
 expected=capture[16+i*9:25+i*9];r=oracle('bookheaders',struct.pack('<Q',1)+packed([book]));assert r.returncode==0 and r.stdout==struct.pack('<Q',1)+expected
 dim=(int.from_bytes(book[:8],'little')>>24)&65535;entries=int.from_bytes(book[:8],'little')>>40
 if dim==65535 and entries==256 and book[8]&1 and not book[8]&4:
  # The first sparse active entry is bit 66 (the low byte bits are order/sparse/used).
  if book[8]&4:(run/'default-rejection.bin').write_bytes(book)
 p=gates/'book-inputs'/f'extent-init-{i}.bin';raw=p.read_bytes();headers,at=unpack(raw);assert at==len(raw)
 first=oracle('headers',raw);a=oracle('pcm',raw);b=oracle('pcm',raw,old)
 accepted=bool(expected[0]);assert first.returncode==a.returncode==b.returncode==(0 if accepted else 1)
 assert a.stdout==b.stdout and a.stderr==b.stderr
 if accepted:assert a.stdout==struct.pack('<I',2)
 else:assert a.stderr.strip()==b'PCM header'
 rows.append({'case':i,'dimensions':dim,'entries':entries,'reference_accepted':accepted,'book_sha256':sha(book),'whole_request_sha256':sha(raw),'header_response_sha256':sha(first.stdout),'fresh_book_response_sha256':sha(r.stdout),'old_new_initialization_identical':True})
# Retain a small ordinary-default regression, with the complete body available.
for book,row in zip(books,rows):
 if row['dimensions']==65535 and row['entries']==256 and book[8]&4:
  (run/'default-rejection.bin').write_bytes(book);break
else:raise AssertionError('missing small baseline')
original=(run/'baseline-golden.libvorbis-headers').read_bytes();assert int.from_bytes(original[8:16],'little')==467 and original[16:]==capture[16:16+467*9]
compatibility=[]
for directory,prefix in [('degenerate-books-20261027','book-golden-'),('residue-geometry-20261027','geometry-golden-')]:
 for p in sorted((root/directory/'golden-inputs').glob(prefix+'*.bin')):
  raw=p.read_bytes();a=oracle('pcm-cases',raw);b=oracle('pcm-cases',raw,old);assert a.returncode==b.returncode==0 and a.stdout==b.stdout
  compatibility.append({'request':p.relative_to(repo).as_posix(),'request_sha256':sha(raw),'response_sha256':sha(a.stdout),'old_new_byte_identical':True})
container=[]
for i in range(3):
 raw=(gates/'configuration-inputs'/f'extent-stream-{i}.bin').read_bytes();headers,at=unpack(raw);assert struct.unpack_from('<QQ',raw,at)==(1,5)
 a=oracle('pcm-cases',raw);assert a.returncode==0;out=a.stdout;assert struct.unpack_from('<IQ',out)==(2,1);at=12;planes=[bytearray(),bytearray()];total=0
 for _ in range(5):
  assert out[at]==ord('P');n,frames=struct.unpack_from('<II',out,at+1);at+=9;total+=frames
  for plane in planes:plane.extend(out[at:at+frames*4]);at+=frames*4
 assert at==len(out) and total==368
 data=(gates/'book-inputs'/f'extent-stream-{i}.ogg').read_bytes();p=run/'independent-stream.ogg';p.write_bytes(data)
 r=subprocess.run([str(exe),'oggpcm',str(p)],capture_output=True);assert r.returncode==0
 out=r.stdout;assert struct.unpack_from('<IIIIQ',out)==(1,83,2,48000,368);at=24;got=[bytearray(),bytearray()];total=0
 while at<len(out):
  link,frames=struct.unpack_from('<II',out,at);at+=8;assert link==0;total+=frames
  for plane in got:plane.extend(out[at:at+frames*4]);at+=frames*4
 assert total==368 and got==planes
 targets=[0,1,79,80,81,207,208,287,288,367,368];q=run/'independent-seeks.bin';q.write_bytes(struct.pack('<Q',len(targets))+b''.join(struct.pack('<QQ',t,368-t) for t in targets))
 r=subprocess.run([str(exe),'oggseek',str(p),str(q)],capture_output=True);assert r.returncode==0;out=r.stdout;assert struct.unpack_from('<IIIIQ',out)==(1,83,2,48000,368);at=24;seeks=[]
 for t in targets:
  position=struct.unpack_from('<Q',out,at)[0];at+=8;got=[bytearray(),bytearray()];total=0
  while True:
   frames=struct.unpack_from('<I',out,at)[0];at+=4
   if not frames:break
   link=struct.unpack_from('<I',out,at)[0];at+=4;assert link==0;total+=frames
   for plane in got:plane.extend(out[at:at+frames*4]);at+=frames*4
  assert position==t and total==368-t and all(a==b[t*4:] for a,b in zip(got,planes));seeks.append({'target':t,'position':position,'frames':total,'raw_suffix_byte_identical':True})
 assert at==len(out);container.append({'stream':i,'sha256':sha(data),'frames':368,'raw_pcm_byte_identical':True,'seeks':seeks})
summary={'scope':'Header-only batch protocol checked against 475 fresh commands and complete headers/synthesis initialization. Existing C PCM modes compared byte for byte to the preserved preceding helper. Three accepted default-limit Ogg probes and 33 C seeks independently agree with raw PCM. One native extent-override stream intentionally exceeds the pinned C guard and is not counted as reference acceptance.','reference_binary_sha256':sha(exe.read_bytes()),'previous_binary_sha256':sha(old.read_bytes()),'golden':{'bytes':len(capture),'sha256':sha(capture),'declarations':475,'accepted':sum(bool(capture[16+i*9]) for i in range(475)),'rejected':sum(not capture[16+i*9] for i in range(475)),'byte_identical':True,'first_467_rows_identical_to_baseline':True},'requests':requests,'whole_initialization':rows,'old_pcm_regressions':compatibility,'container_reference_checks':container}
(run/'independent-reference.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf8',newline='\n')
print('Verified 475 fresh book commands, 475 whole initializations,',len(compatibility),'unchanged C PCM responses, 3 Ogg streams and 33 C seeks.')
