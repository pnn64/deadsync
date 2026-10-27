"""Validate every normative codeword through libmad's public frame API.

Frames put isolated coefficients in subband zero. Compare public hybrid output
with the analytical long IMDCT/window equation; a zero second granule clears
overlap before the next vector. This tests codeword meaning, not native PCM.
"""
import argparse,csv,hashlib,json,math,subprocess
from pathlib import Path

class Bits:
    def __init__(self):self.bits=[]
    def put(self,value,width):
        assert 0<=value<1<<width if width else value==0
        self.bits.extend((value>>n)&1 for n in range(width-1,-1,-1))
    def data(self):
        result=bytearray((len(self.bits)+7)//8)
        for n,bit in enumerate(self.bits):result[n//8]|=bit<<(7-n%8)
        return result

def frame(code,book,extra,signs):
    data=Bits();data.put(code['code'],code['bits'])
    values=[code['x'],code['y']] if book<32 else [(code['y']>>(3-n))&1 for n in range(4)]
    lin=([1,2,3,4,6,8,10,13][book-16] if book<24 else [4,5,6,7,8,9,11,13][book-24]) if 16<=book<32 else 0
    for n,magnitude in enumerate(values):
        if book<32 and magnitude==15 and lin:data.put(extra[n],lin);magnitude+=extra[n]
        if magnitude:data.put((signs>>n)&1,1)
        values[n]=-magnitude if signs&(1<<n) else magnitude
    maximum=max(map(abs,values))
    gain=210 if maximum==0 else math.floor(210+4*math.log2(.5/(maximum**(4/3))))
    side=Bits();side.put(0,9);side.put(0,5);side.put(0,4)
    for granule in range(2):
        side.put(len(data.bits) if granule==0 else 0,12)
        side.put(1 if granule==0 and book<32 else 0,9)
        side.put(gain if granule==0 else 0,8)
        side.put(0,4);side.put(0,1)
        for region in range(3):side.put(book if granule==0 and book<32 and region==0 else 0,5)
        side.put(0,4);side.put(0,3);side.put(0,1);side.put(0,1);side.put(int(book==33) if granule==0 else 0,1)
    assert len(side.bits)==136
    output=bytearray(b'\xff\xfb\xe0\xc0')+side.data()+data.data()
    output.extend(bytes(1044-len(output)))
    return output,values,gain

def main():
    p=argparse.ArgumentParser()
    p.add_argument('--oracle',type=Path,required=True);p.add_argument('--output',type=Path,required=True)
    p.add_argument('--quick',action='store_true')
    args=p.parse_args();args.output.mkdir(parents=True,exist_ok=False)
    crate=Path(__file__).resolve().parents[1]
    codes=[{k:int(v) for k,v in row.items()} for row in csv.DictReader((crate/'tests/data/huffman.csv').open())]
    records=[];stream=args.output/'vectors.mp3'
    with stream.open('wb') as output:
        for book in [0]+[n for n in range(1,34) if n not in [4,14]]:
            base=16 if 16<=book<=23 else 24 if 24<=book<=31 else book
            selected=[r for r in codes if r['book']==base] if book else [{'book':0,'x':0,'y':0,'bits':0,'code':0}]
            if args.quick:selected=selected[:2]
            for row in selected:
                lin=([1,2,3,4,6,8,10,13][book-16] if book<24 else [4,5,6,7,8,9,11,13][book-24]) if 16<=book<32 else 0
                maximum=(1<<lin)-1;extras=[[0,0]]
                if lin and row['x']==15:extras.append([maximum,0])
                if lin and row['y']==15:extras.append([0,maximum])
                if lin and row['x']==15 and row['y']==15:extras.append([maximum,maximum])
                for extra in extras:
                    for signs in range(4 if book<32 else 16):
                        data,values,gain=frame(row,book,extra,signs);output.write(data)
                        records.append({'book':book,'x':row['x'],'y':row['y'],'extra':extra,'signs':signs,'values':values,'gain':gain})
    command=[str(args.oracle.resolve()),str(stream.resolve())]
    with (args.output/'hybrid.tsv').open('wb') as output:
        result=subprocess.run(command,stdout=output,stderr=subprocess.PIPE)
    assert not result.returncode,result.stderr.decode()
    # Fixed before the run: normalized hybrid output, compared with f64 equations.
    rms_limit,max_limit=2e-7,1e-6
    matrix=[[math.sin(math.pi/36*(n+.5))*math.cos(math.pi/72*(2*n+19)*(2*k+1)) for k in range(4)] for n in range(18)]
    errors=[];groups={};count=0
    for case,line in zip(records,(args.output/'hybrid.tsv').open(),strict=True):
        cells=line.strip().split('\t');assert int(cells[0])==count;count+=1
        actual=list(map(float,cells[1:]));assert len(actual)==18
        coefficients=[math.copysign(abs(v)**(4/3),v)*2**((case['gain']-210)/4) for v in case['values']]
        expected=[sum(c*x for c,x in zip(coefficients,row)) for row in matrix]
        delta=[a-b for a,b in zip(actual,expected)];rms=math.sqrt(sum(d*d for d in delta)/18);peak=max(map(abs,delta))
        if rms>rms_limit or peak>max_limit:errors.append(dict(case,rms=rms,max=peak))
        group=groups.setdefault(str(case['book']),{'vectors':0,'max_rms':0,'max_error':0})
        group['vectors']+=1;group['max_rms']=max(group['max_rms'],rms);group['max_error']=max(group['max_error'],peak)
    sha=lambda path:hashlib.sha256(path.read_bytes()).hexdigest()
    report={'passed':not errors,'vectors':count,'groups':groups,'rms_limit':rms_limit,'max_limit':max_limit,
            'errors':errors[:20],'command':command,'oracle_sha256':sha(args.oracle),'vectors_sha256':sha(stream),
            'trace_sha256':sha(args.output/'hybrid.tsv'),'fixture_sha256':sha(crate/'tests/data/huffman.csv'),
            'method':__doc__,'libmad_version':subprocess.check_output(['pkg-config','--modversion','mad'],text=True).strip()}
    (args.output/'results.json').write_text(json.dumps(report,indent=2)+'\n')
    print('vectors',count,'failed',len(errors),'max error',max(g['max_error'] for g in groups.values()),flush=True)
    assert not errors,errors[:2]

if __name__=='__main__':main()
