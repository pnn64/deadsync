from pathlib import Path
import hashlib,json,subprocess,struct
repo=Path.cwd();root=repo/'target/deadlib-audio-reference';run=root/'setup-parity-20261027'
exe=root/'vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe'
sha=lambda data:hashlib.sha256(data).hexdigest()
def read_request(raw):
    at=0
    def u64():
        nonlocal at
        v=int.from_bytes(raw[at:at+8],'little');at+=8;return v
    def packet():
        nonlocal at
        n=u64();v=raw[at:at+n];at+=n;return v
    headers=[packet() for _ in range(3)]
    cases=[]
    for _ in range(u64()):cases.append([packet() for _ in range(u64())])
    assert at==len(raw)
    return headers,cases
def packed(packets):return b''.join(len(p).to_bytes(8,'little')+p for p in packets)
manifest=[];fresh_count=0
paths=sorted((root/'configuration-parity-validation-20261027/configuration-inputs').glob('*.bin'))
assert len(paths)==308
paths+=sorted((run/'golden-inputs').glob('*.bin'))
assert len(paths)==316
for i,p in enumerate(paths):
    headers,cases=read_request(p.read_bytes())
    records=[]
    for index in sorted(set([0,len(cases)//2,len(cases)-1])):
        packets=cases[index]
        case_wire=packed(headers)+struct.pack('<QQ',1,len(packets))+packed(packets)
        (run/'probe-case.bin').write_bytes(case_wire)
        out=subprocess.check_output([str(exe),'pcm-cases',str(run/'probe-case.bin')])
        (run/'probe-fresh.bin').write_bytes(packed(headers+packets))
        fresh=subprocess.check_output([str(exe),'pcm',str(run/'probe-fresh.bin')])
        assert out[:4]==fresh[:4] and out[12:]==fresh[4:],(p,index)
        records.append({'case_index':index,'raw_pcm_sha256':sha(fresh),'samples_protocol_bytes':len(fresh)})
        fresh_count+=1
    manifest.append({'path':p.relative_to(repo).as_posix(),'sha256':sha(p.read_bytes()),'cases':len(cases),'fresh_process_checks':records})
# Appending an unreferenced empty sparse book is accepted by the C reference,
# while the strict native Huffman parser rejects its underpopulated tree.
p=run/'golden-inputs/golden-0.bin';headers,cases=read_request(p.read_bytes())
original=packed(headers)+struct.pack('<Q',len(cases))+b''.join(struct.pack('<Q',len(c))+packed(c) for c in cases)
setup=headers[2];bits=[(b>>i)&1 for b in setup[7:] for i in range(8)]
def bit_fields(fields):return [(v>>i)&1 for v,w in fields for i in range(w)]
assert bits[:8]==bit_fields([(1,8)])
empty=bit_fields([(0x564342,24),(1,16),(1,24),(0,1),(1,1),(0,1),(0,4)])
new=bit_fields([(2,8)])+bits[8:271]+empty+bits[271:];new+=[0]*((-len(new))%8)
headers[2]=setup[:7]+bytes(sum(new[j+i]<<i for i in range(8)) for j in range(0,len(new),8))
mutated=packed(headers)+original[len(packed(read_request(original)[0])):]
q=run/'unused-empty-book.bin';q.write_bytes(mutated)
a=subprocess.check_output([str(exe),'pcm-cases',str(p)]);b=subprocess.check_output([str(exe),'pcm-cases',str(q)])
assert a==b
summary={'scope':'Fresh-process PCM verification of reset-case protocol; every matrix setup checks its first/middle/last case. No floating comparison in this protocol check: bytes must match.','reference_binary_sha256':sha(exe.read_bytes()),'matrix_inputs':308,'golden_inputs':8,'fresh_process_cases':fresh_count,'requests':manifest,'remaining_empty_book_discrepancy':{'scope':'Explicit strict/reference setup difference, excluded from parity totals. Empty trees remain rejected by the native strict parser.','source_request_sha256':sha(p.read_bytes()),'mutated_request_sha256':sha(mutated),'setup_sha256':sha(headers[2]),'libvorbis_accepts':True,'reference_pcm_identical':True}}
(run/'independent-reference.json').write_text(json.dumps(summary,indent=2)+'\n',encoding='utf8',newline='\n')
print('Checked',fresh_count,'fresh-process cases over',len(paths),'setups; C empty-book permissiveness separately reproduced.')
