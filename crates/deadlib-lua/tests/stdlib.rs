use deadlib_lua::{Value, Vm, compile};
use lua_reference::{Lua, MultiValue, Value as Ref};

#[derive(Debug, PartialEq)]
enum Scalar {
    Nil,
    Bool(bool),
    Number(u64),
    NaN,
    Bytes(Vec<u8>),
}
fn number(n: f64) -> Scalar {
    if n.is_nan() {
        Scalar::NaN
    } else {
        Scalar::Number(n.to_bits())
    }
}
fn compare(source: &[u8]) {
    let mut vm = Vm::new();
    vm.set_output_capture(true);
    let p = compile(source)
        .unwrap_or_else(|e| panic!("compile {e}: {}", String::from_utf8_lossy(source)));
    let values = vm
        .execute(&p)
        .unwrap_or_else(|e| panic!("execute {e}: {}", String::from_utf8_lossy(source)))
        .to_vec();
    let actual: Vec<_> = values
        .into_iter()
        .map(|v| match v {
            Value::Nil => Scalar::Nil,
            Value::Boolean(b) => Scalar::Bool(b),
            Value::Number(n) => number(n),
            Value::String(id) => Scalar::Bytes(vm.string_bytes(id).unwrap().to_vec()),
            _ => panic!("return scalar observations"),
        })
        .collect();
    let lua = Lua::new();
    let expected: Vec<_> = lua
        .load(source)
        .set_name("=(deadlib-lua)")
        .eval::<MultiValue>()
        .unwrap()
        .into_iter()
        .map(|v| match v {
            Ref::Nil => Scalar::Nil,
            Ref::Boolean(b) => Scalar::Bool(b),
            Ref::Integer(n) => number(n as f64),
            Ref::Number(n) => number(n),
            Ref::String(s) => Scalar::Bytes(s.as_bytes().to_vec()),
            _ => panic!("return reference scalars"),
        })
        .collect();
    assert_eq!(actual, expected, "{}", String::from_utf8_lossy(source));
}

#[test]
fn byte_string_primitives_and_methods() {
    for source in [
        "return string.len('a\\0b'), ('a\\0b'):len(), string.reverse('a\\0b'), string.lower('AbC'),string.upper('aBc')",
        "return string.sub('abcdef',2,-2),string.sub('abcdef',-99,99),string.sub('abcdef',99),string.sub('abcdef',0,0)",
        "return string.byte('abc',-5), string.byte('abc',1,-9),string.byte('a\\0z',1,3)",
        "return string.byte('abc',-1), string.byte('abc',-9,0),string.char(0,1,65,255)",
        "return string.rep('ab',3), string.rep('ab',0),string.rep('ab',-1),string.rep(3,3)",
        "return string.sub(12345,2,4), string.byte(12345,2,4)",
    ] {
        compare(source.as_bytes());
    }
}

#[test]
fn patterns_captures_backtracking_and_boundaries() {
    for source in [
        "return string.find('hello world','world'),string.match('hello world','(%a+)%s+(%a+)')",
        "return string.match('aaaab','(a*)(a+)b'),string.match('abc123xyz','(%a-)(%d+)(%a+)')",
        "return string.find('abc','',100),string.find('abc','$',100),string.find('xxabc','^abc',3)",
        "return string.match('xyzabc','^abc',4),string.match('abc','()(%a+)()')",
        "return string.match('abc abc','(%a+)%s+%1'),string.match('abc abc','(%a+)%s+%2')",
        "return string.match('x(a(b)c)y','%b()'),string.match('x\"a\"y','%b\"\"')",
        "return string.find('ab cd','%f[%a]cd'),string.find('ab cd','%f[%z]'),string.find('abc','%f[^%z]')",
        "return string.match('az09_!','([%w_]+)'),string.match('Az!','(%A+)'),string.match('gG','%g%G')",
        "return string.match('a\\0b','a%zb'),string.find('a\\0.b','a\\0.b'),string.find('a\\0.b','.\\0.b')",
        "return string.match(']', '[]]'),string.match('a','[^]]'),string.match('-','[a-]')",
        "return string.match('aab','a?ab'),string.match('aaab','a-b'),string.match('**','**')",
        "return pcall(string.match,'x','a%')",
        "return pcall(string.match,'a','a%')",
        "return pcall(string.match,'x','a[')",
        "return pcall(string.match,'a','a[')",
        "return pcall(string.match,'abc','(%a+')",
        "return pcall(string.match,'abc','()%1')",
        "return pcall(string.match,'abc','%0')",
        "return pcall(string.match,'abc','%fabc')",
        "return pcall(string.match,'abc','%b(')",
        "return pcall(string.match,'abc',')')",
        "return pcall(string.match,'abc','[]')",
    ] {
        // The deliberately invalid capture is observed through pcall below.
        let source=source.replace("return string.match('abc abc','(%a+)%s+%1'),string.match('abc abc','(%a+)%s+%2')",
            "return string.match('abc abc','(%a+)%s+%1'),pcall(string.match,'abc abc','(%a+)%s+%2')");
        compare(source.as_bytes());
    }
}

#[test]
fn pattern_iterators_have_independent_state_and_empty_matches() {
    for source in [
        "local s=''; for word in string.gmatch('one two 3','%w+') do s=s..'['..word..']' end; return s",
        "local s='';for p,v in ('abc'):gmatch('()(.?)') do s=s..p..v end; return s",
        "local a=string.gmatch('abc','.');local b=string.gmatch('12','.');return a(),b(),a(),b(),a(),a()",
        "local f=string.gmatch('a^b','^');return f(),f()",
        "local f=string.gmatch('abc','');return f(),f(),f(),f(),f()",
    ] {
        compare(source.as_bytes());
    }
}

#[test]
fn replacements_preserve_values_callbacks_and_error_unwinding() {
    for source in [
        "return string.gsub('abc abc','(%a+)','[%1:%0]'),string.gsub('abc','','-')",
        "return string.gsub('aaa','a*','x'),string.gsub('abc','^','x'),string.gsub('abc','$','x')",
        "return string.gsub('abc','(.)','%q%'),string.gsub('abc','(.)',123,2)",
        "return string.gsub('abc','(.)',function(s) if s=='a' then return false elseif s=='b' then return nil else return 9 end end)",
        "return string.gsub('a1b2','(%a)(%d)',function(a,b) return b..a end)",
        "return string.gsub('abc','(.)',{a='A',b=false,c=12})",
        "local t=setmetatable({},{__index=function(t,k) return '['..k..']' end});return string.gsub('abc','.',t)",
        "return string.gsub('abc','()',{[1]='x',[2]='y',[3]='z',[4]='!'})",
        "return string.gsub('ab','.(', 'x'),string.gsub('abc','[','x',0)",
        "return string.gsub('ab','.',function(s) return string.gsub(s,'(.)','%1%1') end)",
        "return string.gsub('ab','.',function(s)local ok=pcall(string.gsub,s,'.',function()error(7)end);return s..tostring(ok) end)",
        "local ok,e=pcall(string.gsub,'abc','.',function() error(17) end);return ok,e,string.gsub('a','.','x')",
        "return pcall(string.gsub,'abc','.',function() return true end)",
        "return pcall(string.gsub,'abc','(.)','%2')",
        "local n=0;local ok,e=pcall(function()return string.gsub('ab','.',function() n=n+1;if n==2 then error('stop',0) end;return 'x' end)end);return ok,e,n",
    ] {
        compare(source.as_bytes());
    }
}

#[test]
fn formatted_strings_and_platform_integer_width() {
    for source in [
        "return string.format('%%:%s:%d:%i:%o:%u:%x:%X','hi',-123,12,31,-1,255,255)",
        "return string.format('%+08d|%-8.5d|% 8d|%#08x|%#.0o|%.0d',12,12,12,31,0,0)",
        "return string.format('%q','a\\0\\r\\n\\\"\\\\z'),string.format('[%6.3s][%-6s]','abcdef','ab')",
        "return string.format('[%s][%.5s][%5c][%-5c]','a\\0b','a\\0b',0,0)",
        "return string.format('%s',string.rep('a',100)..'\\0b'),string.format('%99s',string.rep('a',100))",
        "return string.format('%c%c%c',65,255,321),string.format('%d %u %x',2147483648,4294967295,4294967296)",
        "return string.format('%f|%.0f|%#.0f|%.3e|%E|%g|%G',1.25,2.5,2.5,0.00125,12,1e9,1e-9)",
        "return string.format('%+010.2f|%#.5g|%.1g|%.2g|%.6g|%.6g',-1.25,12,9.99,0.00009999,999999.5,1000000)",
        "return string.format('%g|%f|%e|%+g|%G',-0,1/0,-1/0,0/0,1/0)",
        "return string.format('%c%c%c%c',2147483648,4294967296,1/0,0/0),tostring(0/0),tostring(-(0/0))",
        "return pcall(string.format,'%100d',1)",
        "return pcall(string.format,'%------d',1)",
        "return pcall(string.format,'%a',1)",
    ] {
        compare(source.as_bytes());
    }
}

#[test]
fn table_operations_are_raw_and_callbacks_can_stop_or_mutate() {
    for source in [
        "local t={1,2,3};table.insert(t,2,9);local a=table.remove(t);return table.concat(t,':'),a,table.getn(t),table.maxn(t)",
        "local t={1,2};table.insert(t,0,9);return t[0],t[1],t[2],t[3],select('#',table.remove(t,0)),table.remove(t,9)",
        "local t={1,2};table.insert(t,7,9);return t[7],t[3],table.maxn(t)",
        "local t=setmetatable({'a','b'},{__len=function()return 99 end,__index=function()return 'x' end,__newindex=function()error('bad')end});table.insert(t,'c');return table.concat(t,'\\0'),table.getn(t),table.remove(t)",
        "return table.concat({[0]='a',[1]=12,[2]='b'},3,0,2),table.concat({},nil,5,1),table.maxn({[-1]=true,[3.5]=true,['9']=true})",
        "local t={1,2,3};local s='';local r=table.foreachi(t,function(k,v)s=s..k..v;if k==1 then t[3]=7;t[4]=8 end end);return s,r",
        "local t={1,2,3};local n=0;local r=table.foreach(t,function(k,v)n=n+v;t[k]=nil end);return n,r,next(t)",
        "local t={1,2,3};local n=0;local r=table.foreachi(t,function()n=n+1;return false end);return r,n",
        "local t={1,2,3};return table.foreach(t,function()return 7,8 end)",
        "return pcall(table.concat,{1,false})",
        "return pcall(table.setn,{})",
    ] {
        compare(source.as_bytes());
    }
}

#[test]
fn table_sort_preserves_comparison_order_and_callback_mutations() {
    for source in [
        "local t={5,2,4,1,3,0,2};table.sort(t);return table.concat(t,',')",
        "local t={5,2,4,1,3,0,2};local s='';table.sort(t,function(a,b)s=s..a..'<'..b..';';return a<b end);return s,table.concat(t,',')",
        "local t={8,7,6,5,4,3,2,1};local n=0;table.sort(t,function(a,b)n=n+1;if n==2 then t[8]=9 end;return a<b end);return n,table.concat(t,',')",
        "local t={3,2,1};local s='';table.sort(t,function(a,b)s=s..a..b;return a>b end);return s,table.concat(t,',')",
        "local m={__lt=function(a,b)return a.n<b.n end};local t={setmetatable({n=3},m),setmetatable({n=1},m),setmetatable({n=2},m)};table.sort(t);return t[1].n,t[2].n,t[3].n",
        "local t={4,2,1,3};table.sort(t,function(a,b)local x={2,1};table.sort(x);return a<b end);return table.concat(t,',')",
        "local t={3,1,2};local ok,e=pcall(table.sort,t,function()error(9)end);table.sort(t);return ok,e,table.concat(t,',')",
        "return pcall(table.sort,{1,2,3,4,5},function()return true end)",
    ] {
        compare(source.as_bytes());
    }
    // Every permutation through length six checks the observable comparator trace.
    compare(br#"
        local count,hash=0,0
        local function permutations(t,n)
            if n==0 then
                local c={};for i=1,#t do c[i]=t[i] end
                table.sort(c,function(a,b) count=count+1;hash=(hash*31+a*7+b)%10000019;return a<b end)
                for i=1,#c do assert(c[i]==i) end
            else for i=1,n do t[i],t[n]=t[n],t[i];permutations(t,n-1);t[i],t[n]=t[n],t[i] end end
        end
        for n=0,6 do local t={};for i=1,n do t[i]=i end;permutations(t,n) end
        return count,hash
    "#);
}

fn quoted(bytes: &[u8]) -> String {
    let mut out = String::from("'");
    for b in bytes {
        out.push_str(&format!("\\{b:03}"));
    }
    out.push('\'');
    out
}

#[test]
fn generated_byte_patterns_match_reference_results_and_errors() {
    let mut seed = 0x8b5ad4cef9031782u64;
    let mut random = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed as usize
    };
    let atoms = b"abz019\0%()[]^$+-*?.fscwx";
    for _ in 0..4000 {
        let size = random() % 21;
        let pattern: Vec<_> = (0..size).map(|_| atoms[random() % atoms.len()]).collect();
        let size = random() % 17;
        let subject: Vec<_> = (0..size)
            .map(|_| b"abz019\0() []%"[random() % 13])
            .collect();
        let source = format!(
            "return pcall(string.find,{},{})",
            quoted(&subject),
            quoted(&pattern)
        );
        compare(source.as_bytes());
    }
}

#[test]
fn generated_float_formats_match_reference_bytes() {
    let mut seed = 0x197823cf615aa072u64;
    for _ in 0..2000 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let conversion = b"eEfgG"[(seed as usize >> 8) % 5] as char;
        let flags = ["", "-", "+", " ", "#", "0", "+#0", "-#"][seed as usize % 8];
        let precision = (seed >> 16) % 18;
        let width = (seed >> 24) % 30;
        let value = (seed as i64 as f64) / 10.0f64.powi(((seed >> 32) % 35) as i32);
        let source =
            format!("return string.format('%{flags}{width}.{precision}{conversion}',{value:e})");
        compare(source.as_bytes());
    }
}
