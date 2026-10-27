from pathlib import Path
import hashlib, json, subprocess

repo=Path.cwd();root=repo/'target/deadlib-audio-reference';run=root/'concealment-parity-20261027'
exe=root/'vorbis-oracle-build/Release/deadlib_vorbis_oracle.exe'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
def packets(path):
    data=path.read_bytes();at=0;partial=bytearray();out=[]
    while at<len(data):
        assert data[at:at+4]==b'OggS'
        count=data[at+26];laces=data[at+27:at+27+count];body=at+27+count
        for size in laces:
            partial.extend(data[body:body+size]);body+=size
            if size<255:out.append(bytes(partial));partial.clear()
        at=body
    assert not partial
    return out
def encode(items):return b''.join(len(x).to_bytes(8,'little')+x for x in items)
counter=0
def call(mode,request):
    global counter
    p=run/f'oracle-selfcheck-{counter}.bin';counter+=1;p.write_bytes(request)
    q=subprocess.run([str(exe),mode,str(p)],capture_output=True);assert q.returncode==0,q.stderr
    return q.stdout
records=[]
paths=[repo/'crates/deadlib-audio/decode/vorbis/tests/fixtures'/name for name in ['mono.ogg','stereo.ogg','eight-channel.ogg','surround.ogg']]
paths.append(root/'corpus/floor0/vorbis_floor_type_0.ogg')
for path in paths:
    p=packets(path);head=encode(p[:3]);audio=p[3:]
    cases=[[b''],[b'\x01'],audio[:3],[audio[0],b'',*audio[2:5]]]
    if len(audio)>30:cases.append(audio[23:29])
    expected=[call('pcm',head+encode(case)) for case in cases]
    channels=expected[0][:4];assert all(x[:4]==channels for x in expected)
    request=head+len(cases).to_bytes(8,'little')+b''.join(len(case).to_bytes(8,'little')+encode(case) for case in cases)
    actual=call('pcm-cases',request)
    assert actual[:4]==channels and int.from_bytes(actual[4:12],'little')==len(cases)
    assert actual[12:]==b''.join(x[4:] for x in expected)
    records.append({'path':path.relative_to(repo).as_posix(),'sha256':sha(path),'cases':len(cases),'fresh_process_pcm_equals_restarted_case_mode_byte_for_byte':True,'request_sha256':hashlib.sha256(request).hexdigest(),'response_sha256':hashlib.sha256(actual).hexdigest(),'response_bytes':len(actual)})
m={'binary_sha256':sha(exe),'oracle_source_sha256':sha(repo/'crates/deadlib-audio/decode/vorbis/tests/oracle/oracle.c'),'files':records,'fresh_processes_compared':sum(x['cases'] for x in records)}
(run/'oracle-selfcheck.json').write_text(json.dumps(m,indent=2)+'\n',encoding='utf8',newline='\n')
print('Reference case-mode output matches',m['fresh_processes_compared'],'fresh-process PCM runs byte for byte.')
