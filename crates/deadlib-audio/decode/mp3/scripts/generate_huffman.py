"""Generate a native packed trie from ISO's numeric Layer III huffcode data.

Only table definitions are consumed; no reference implementation code is read
or translated. Duplicate prefixes, incomplete trees and malformed tables fail.
"""
import argparse,csv,hashlib,json
from pathlib import Path

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--input',type=Path,required=True)
    parser.add_argument('--crate',type=Path,default=Path(__file__).resolve().parents[1])
    args=parser.parse_args();tables={};current=None
    for original in args.input.read_text(encoding='ascii').splitlines():
        line=original.split('#',1)[0].strip()
        if not line:continue
        fields=line.split()
        if fields[0]=='.table':
            number,x,y,lin=map(int,fields[1:]);assert number not in tables
            current={'number':number,'x':x,'y':y,'linbits':lin,'rows':[],'reference':None};tables[number]=current
        elif fields[0] in ['.reference','.referenz']:current['reference']=int(fields[1])
        elif fields[0]=='.end':break
        else:
            assert current is not None
            if current['number']<32:x,y,length=map(int,fields[:3]);code=fields[3]
            else:x=0;y,length=map(int,fields[:2]);code=fields[2]
            assert len(code)==length and set(code)<={'0','1'},original
            current['rows'].append((x,y,length,int(code,2)))
    assert set(tables)==set(range(34))
    nodes=[];roots=[65535]*34;fixtures=[]
    for number,t in tables.items():
        if t['reference'] is not None:
            source=tables[t['reference']];assert source['reference'] is None
            assert (t['x'],t['y'])==(source['x'],source['y']);roots[number]=roots[t['reference']];continue
        if not t['rows']:
            assert number in [0,4,14];continue
        assert len(t['rows'])==t['x']*t['y']
        assert len({(x,y) for x,y,_,_ in t['rows']})==len(t['rows'])
        if number==33:
            # Table B is the complement of its fixed four-bit code; no trie.
            assert all(x==0 and length==4 and code==15^y for x,y,length,code in t['rows'])
            fixtures.extend((number,x,y,length,code) for x,y,length,code in t['rows'])
            continue
        if number==32:
            # The all-zero quadruple is a single one bit. Other codes start
            # with zero and have at most five remaining bits.
            count1=[None]*32
            for x,y,length,code in t['rows']:
                assert x==0 and 0<=y<16 and 1<=length<=6
                if y==0:
                    assert length==1 and code==1
                else:
                    assert length>=2 and code<(1<<(length-1))
                    used=length-1
                    for suffix in range(1<<(5-used)):
                        index=(code<<(5-used))|suffix
                        assert count1[index] is None,'duplicate count1 prefix'
                        count1[index]=(used<<8)|y
            assert all(entry is not None for entry in count1),'incomplete count1 prefixes'
            fixtures.extend((number,x,y,length,code) for x,y,length,code in t['rows'])
            continue
        root=len(nodes);nodes.append([65535,65535]);roots[number]=root
        for x,y,length,code in t['rows']:
            assert 0<=x<t['x'] and 0<=y<t['y'] and length<=19
            token=(x<<4)|y if number<32 else y
            position=root
            for shift in range(length-1,-1,-1):
                bit=(code>>shift)&1
                if shift==0:
                    assert nodes[position][bit]==65535,'duplicate code/prefix'
                    nodes[position][bit]=32768|token
                else:
                    child=nodes[position][bit]
                    if child==65535:
                        child=len(nodes);nodes.append([65535,65535]);nodes[position][bit]=child
                    assert child<32768,'prefix is a leaf'
                    position=child
            fixtures.append((number,x,y,length,code))
    assert len(fixtures)==1410 and len(nodes)==1363
    assert all(65535 not in node for node in nodes),'incomplete prefix tree'
    prefix=[]
    for value in range(32):
        node=roots[24]
        for used in range(1,6):
            child=nodes[node][(value>>(5-used))&1]
            if child&32768:
                prefix.append(child|(used<<8));break
            node=child
        else:prefix.append(node)
    # Complete book-1 code/sign words fit in five bits. Each packed entry
    # stores signed x/y bytes and its total consumed length.
    assert tables[1]['linbits']==0 and tables[1]['reference'] is None
    assert all(t['reference']!=1 for t in tables.values())
    book1=[None]*32
    for x,y,length,code in tables[1]['rows']:
        assert 0<=x<=1 and 0<=y<=1
        count=(x!=0)+(y!=0);used=length+count
        assert 1<=used<=5
        for signs in range(1<<count):
            remaining=count;values=[]
            for magnitude in [x,y]:
                if magnitude:
                    remaining-=1
                    values.append(-magnitude if signs&(1<<remaining) else magnitude)
                else:values.append(0)
            packed=(used<<16)|(values[0]&255)|((values[1]&255)<<8)
            word=(code<<count)|signs
            for suffix in range(1<<(5-used)):
                index=(word<<(5-used))|suffix
                assert book1[index] is None,'duplicate signed book-1 prefix'
                book1[index]=packed
    assert all(entry is not None for entry in book1),'incomplete signed book-1 prefixes'
    roots[1]=65535
    needed=set()
    def retain(node):
        if node==65535 or node&32768 or node in needed:return
        needed.add(node)
        for child in nodes[node]:retain(child)
    for root in roots[:24]:retain(root)
    for entry in prefix:retain(entry)
    mapping={node:index for index,node in enumerate(sorted(needed))}
    def remap(entry):return entry if entry&32768 else mapping[entry]
    nodes=[[remap(child) for child in nodes[node]] for node in sorted(needed)]
    roots=[65535 if root==65535 else remap(root) for root in roots[:24]]
    prefix=[remap(entry) for entry in prefix]
    sign_layout=[];sign_outputs=[]
    for value in range(16):
        count=value.bit_count()
        sign_layout.append((len(sign_outputs)<<3)|count)
        for signs in range(1<<count):
            remaining=count;output=[]
            for bit in [8,4,2,1]:
                if value&bit:
                    remaining-=1
                    output.append(-1 if signs&(1<<remaining) else 1)
                else:output.append(0)
            sign_outputs.append(output)
    assert len(sign_outputs)==81
    # Table B's complete code/sign words fit in eight bits. Reuse the compact
    # signed-output records; store only their index and the consumed length.
    count1b=[None]*256
    for x,value,length,code in tables[33]['rows']:
        count=value.bit_count();used=length+count
        assert x==0 and 4<=used<=8
        for signs in range(1<<count):
            entry=(used<<8)|((sign_layout[value]>>3)+signs)
            word=(code<<count)|signs
            for suffix in range(1<<(8-used)):
                index=(word<<(8-used))|suffix
                assert count1b[index] is None,'duplicate signed count1-B prefix'
                count1b[index]=entry
    assert all(entry is not None for entry in count1b),'incomplete signed count1-B prefixes'
    lines=['// Numeric ISO Layer III codebooks; generated by scripts/generate_huffman.py.',
           '// Source hashes and provenance: HUFFMAN.md. No reference decoder code.',
           '#[rustfmt::skip]',f'pub(super) static ROOTS: [u16; 24] = {roots!r};',
           '#[rustfmt::skip]',f'pub(super) static PAIR1: [u32; 32] = {book1!r};',
           '#[rustfmt::skip]',f'pub(super) static LINBITS: [u8; 32] = {[tables[n]["linbits"] for n in range(32)]!r};',
           '#[rustfmt::skip]',f'pub(super) static COUNT1: [u16; 32] = {count1!r};',
           '#[rustfmt::skip]',f'pub(super) static COUNT1_B: [u16; 256] = {count1b!r};',
           '#[rustfmt::skip]',f'pub(super) static ESCAPE_PREFIX: [u16; 32] = {prefix!r};',
           '#[rustfmt::skip]',f'pub(super) static NODES: [[u16; 2]; {len(nodes)}] = [']
    for start in range(0,len(nodes),6):lines.append('    '+', '.join(f'[{a}, {b}]' for a,b in nodes[start:start+6])+',')
    lines.append('];')
    lines+=['#[rustfmt::skip]',f'pub(super) static SIGN_LAYOUT: [u16; 16] = {sign_layout!r};',
            '#[rustfmt::skip]',f'pub(super) static COUNT1_SIGNS: [[i16; 4]; {len(sign_outputs)}] = [']
    for start in range(0,len(sign_outputs),4):
        lines.append('    '+', '.join(repr(row) for row in sign_outputs[start:start+4])+',')
    lines.append('];')
    (args.crate/'src/huffman_tables.rs').write_text('\n'.join(lines)+'\n',encoding='utf-8',newline='\n')
    folder=args.crate/'tests/data';folder.mkdir(exist_ok=True)
    with (folder/'huffman.csv').open('w',encoding='ascii',newline='') as output:
        writer=csv.writer(output,lineterminator='\n');writer.writerow(['book','x','y','bits','code']);writer.writerows(fixtures)
    manifest={'source_sha256':hashlib.sha256(args.input.read_bytes()).hexdigest(),'symbols':len(fixtures),'nodes':len(nodes),'book1_prefix_entries':len(book1),'count1_sign_outputs':len(sign_outputs),'count1_b_prefix_entries':len(count1b),
              'tables':[{k:v for k,v in t.items() if k!='rows'} for t in tables.values()]}
    (folder/'huffman.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf-8',newline='\n')
    print(len(fixtures),'symbols;',len(nodes),'complete trie nodes')

if __name__=='__main__':main()
