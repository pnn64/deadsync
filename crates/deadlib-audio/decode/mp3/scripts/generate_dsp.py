"""Native transform matrices from MPEG equations and numeric synthesis window."""
import argparse,hashlib,json,math,re
from pathlib import Path

def literal(value):
    # Shortest decimal that roundtrips to the exact generated f64 value.
    text=repr(float(value))
    return text if '.' in text or 'e' in text else text+'.0'

def main():
    p=argparse.ArgumentParser();p.add_argument('--dewindow',type=Path,required=True)
    p.add_argument('--crate',type=Path,default=Path(__file__).resolve().parents[1]);a=p.parse_args()
    rows=re.findall(r'D\[\s*(\d+)\s*\]\s*=\s*([-+0-9.]+)',a.dewindow.read_text(encoding='ascii'))
    assert [int(n) for n,_ in rows]==list(range(512))
    window=[float(v) for _,v in rows]
    long=[[math.cos(math.pi/72*(2*n+19)*(2*k+1)) for k in range(18)] for n in range(36)]
    short=[[math.cos(math.pi/24*(2*n+7)*(2*k+1)) for k in range(6)] for n in range(12)]
    synth=[[math.cos(math.pi/64*(16+n)*(2*k+1)) for n in range(64)] for k in range(32)]
    windows=[]
    for kind in range(4):
        values=[]
        for n in range(36):
            value=math.sin(math.pi/36*(n+.5))
            if kind==1:
                if n>=30:value=0
                elif n>=24:value=math.sin(math.pi/12*(n-18+.5))
                elif n>=18:value=1
            elif kind==3:
                if n<6:value=0
                elif n<12:value=math.sin(math.pi/12*(n-6+.5))
                elif n<18:value=1
            elif kind==2:value=0 # short transforms use their own 12-point window
            values.append(value)
        windows.append(values)
    ci=[-.6,-.535,-.33,-.185,-.095,-.041,-.0142,-.0037]
    cs=[1/math.sqrt(1+x*x) for x in ci];ca=[x/math.sqrt(1+x*x) for x in ci]
    lines=['// Generated from MPEG transform equations; see SYNTHESIS.md.',
           '// The synthesis-window input is numeric ISO table data only.',
           '// Preserve evaluated cosine coefficients rather than substitute constants.',
           '#![allow(clippy::approx_constant)]']
    for name,data in [('IMDCT_LONG',long),('IMDCT_SHORT',short),('SYNTHESIS',synth),('WINDOW_LONG',windows)]:
        lines+=['#[rustfmt::skip]',f'pub(super) static {name}: [[f64; {len(data[0])}]; {len(data)}] = [']
        lines+=['    ['+', '.join(map(literal,row))+'],' for row in data];lines+= ['];']
    for name,data in [('WINDOW_SHORT',[math.sin(math.pi/12*(n+.5)) for n in range(12)]),('ALIAS_CS',cs),('ALIAS_CA',ca),('DEWINDOW',window)]:
        lines+=['#[rustfmt::skip]',f'pub(super) static {name}: [f64; {len(data)}] = [']
        lines+=['    '+', '.join(map(literal,data[n:n+8]))+',' for n in range(0,len(data),8)];lines+= ['];']
    (a.crate/'src/dsp_tables.rs').write_text('\n'.join(lines)+'\n',encoding='utf-8',newline='\n')
    record={'dewindow_sha256':hashlib.sha256(a.dewindow.read_bytes()).hexdigest(),
            'method':'f64 MPEG cosine/long/start/stop/short window equations and normalized alias coefficients; original numeric dewindow input only.',
            'matrices':{'long':[36,18],'short':[12,6],'synthesis':[64,32]},'synthesis_window':512}
    (a.crate/'tests/data/dsp.json').write_text(json.dumps(record,indent=2)+'\n',encoding='utf-8',newline='\n')
    print('Generated native transform matrices and 512 synthesis-window coefficients')

if __name__=='__main__':main()
