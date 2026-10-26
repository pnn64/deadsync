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

#[test]
fn locale_queries_and_validation_keep_the_c_locale() {
    compare(br#"
      assert(os.setlocale('C'))
      local out={os.setlocale(),os.setlocale(nil,'numeric'),os.setlocale('C','ctype')}
      for _,args in ipairs({{false},{true,'bad'},{'C','bad'},{'C',false},{'C','NUMERIC'},
        {'a locale which does not exist'},{'C\0ignored','numeric\0ignored'}})do
        local ok,value=pcall(os.setlocale,unpack(args));out[#out+1]=tostring(ok);out[#out+1]=tostring(value)
      end
      out[#out+1]=os.setlocale(nil,'all');return unpack(out)
    "#);
}

#[test]
fn runtime_hexadecimal_fractions_round_like_native_conversions() {
    for text in [
        "0x1.8p1",
        "-0x.8p+2",
        "+0Xab.cdP-3",
        "0x0.0p-99999",
        "0x1p99999",
        "0x1p-99999",
        "0x1.fffffffffffffp-1023",
        "0x.0000000000001p-1022",
        "0x1.00000000000008p0",
        "0x1.0000000000000800000001p0",
        "0x.",
        "0x..1",
        "0x1p",
        "0x1p+",
        "0x1.2.3",
    ] {
        compare(format!("local n=tonumber('{text}');return n,n and 1/n").as_bytes());
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
fn select_and_unpack_narrow_native_integer_arguments() {
    for index in [
        "4294967297",
        "4294967298",
        "-4294967297",
        "2147483648",
        "0/0",
        "1/0",
        "-1/0",
        "9223372036854775808",
        "'4294967297'",
        "-0.9",
    ] {
        compare(format!("local function pack(...)return select('#',...),... end;return pack(pcall(select,{index},'a',nil,false,'d'))").as_bytes());
        compare(format!("local t={{[0]='zero',[1]='one',[2]='two',[3]='three'}};return pcall(unpack,t,{index},{index})").as_bytes());
    }
}

#[test]
fn character_arguments_use_native_int_narrowing() {
    for value in [
        "4294967296",
        "4294967361",
        "-4294967231",
        "4294967551",
        "4294967552",
        "-4294967297",
        "0/0",
        "1/0",
        "-1/0",
        "9223372036854775808",
        "'4294967361'",
        "65.9",
    ] {
        compare(format!("return pcall(string.char,{value})").as_bytes());
    }
}

#[test]
fn native_result_limits_are_catchable_and_include_unused_arguments() {
    for extra in 0..=3 {
        let extra_args = ",true".repeat(extra);
        for count in 7994..=8001 {
            // Summarize in Lua before pcall returns: the development oracle's
            // API checks reject pcall's own status push after huge open results.
            compare(format!("local function summarize(...)return select('#',...),select(1,...),select(-1,...)end;return pcall(function()return summarize(unpack({{[1]='first',[{count}]='last'}},1,{count}{extra_args}))end)").as_bytes());
            compare(format!("local s=string.rep('a',{count});local function summarize(...)return select('#',...),select(1,...),select(-1,...)end;return pcall(function()return summarize(string.byte(s,1,{count}{extra_args}))end)").as_bytes());
        }
    }
}

#[test]
fn repeated_strings_narrow_counts_before_allocating() {
    for value in [
        "4294967296",
        "4294967297",
        "4294967299",
        "-4294967295",
        "-4294967297",
        "0/0",
        "1/0",
        "-1/0",
        "9223372036854775808",
        "'4294967297'",
        "3.9",
    ] {
        compare(format!("return pcall(string.rep,'xy',{value})").as_bytes());
    }
}

#[test]
fn generated_native_narrowing_preserves_results_errors_and_numeric_strings() {
    let mut inputs = Vec::new();
    for high in [-1_048_576_i64, -65_536, -1, 0, 1, 65_536, 1_048_576] {
        for low in [-7, -1, 0, 1, 2, 3, 32, 255, 256] {
            let value = high * 4_294_967_296 + low;
            inputs.push(value.to_string());
            inputs.push(format!("'{value}'"));
        }
    }
    compare(format!(r#"local out={{}};local function save(...)out[#out+1]=tostring(select('#',...));for i=1,select('#',...)do out[#out+1]=tostring(select(i,...))end end;local t={{[0]='zero','one','two','three'}};for _,x in ipairs({{{}}})do save(pcall(select,x,'a',nil,false));save(pcall(unpack,t,x,x+2));save(pcall(string.char,x));save(pcall(string.rep,'xy',x))end;return table.concat(out,'|')"#,inputs.join(",")).as_bytes());
}

#[test]
fn unpack_defaults_empty_ranges_and_overflow_keep_native_error_precedence() {
    for count in 7997..=8001 {
        compare(format!("local t={{}};for i=1,{count} do t[i]=i end;local function summarize(...)return select('#',...),select(1,...),select(-1,...)end;return pcall(function()return summarize(unpack(t))end)").as_bytes());
    }
    for arguments in [
        "{},3,1",
        "{},0/0,0/0",
        "{},-2147483648,2147483647",
        "{},0,2147483647",
        "{},2147483647,-2147483648",
        "{},nil,false",
        "false,false,false",
        "{},false,false",
    ] {
        compare(format!("return pcall(function()return unpack({arguments})end)").as_bytes());
    }
}

#[test]
fn maximum_unpack_results_remain_live_during_collection() {
    compare(br#"local function build()local t={};for i=1,7999 do t[i]={i}end;return unpack(t)end;local function check(...)collectgarbage();local t={...};return #t,t[1][1],t[#t][1]end;return check(build())"#);
}

#[test]
fn table_concat_wide_intervals_preserve_first_value_errors() {
    for arguments in [
        "{},'',-2147483648,2147483647",
        "{false},'',1,10000002",
        "{'a'},'',1,10000002",
        "{},false,1,10000002",
        "{},'',2147483647,-2147483648",
    ] {
        compare(format!("return pcall(table.concat,{arguments})").as_bytes());
    }
}

#[test]
fn table_insert_accepts_a_large_negative_position_without_an_invented_limit() {
    compare(br#"local t=setmetatable({4,5},{__newindex=function()error('metamethod')end});table.insert(t,-10000001,'x');return t[-10000001],t[-10000000],t[0],t[1],t[2],t[3]"#);
}

#[test]
fn sort_reserves_native_stack_before_comparator_validation() {
    for count in 7957..=7962 {
        for comparator in ["nil", "false"] {
            compare(format!("local args={{}};for i=1,{count} do args[i]=true end;return pcall(function()return table.sort({{}},{comparator},unpack(args))end)").as_bytes());
        }
    }
}

#[test]
fn sort_discards_unused_argument_roots_before_comparison() {
    compare(br#"local weak=setmetatable({},{__mode='v'});local function extra()local p={};weak[1]=p;return p end;local gone;local t={2,1};table.sort(t,function(a,b)collectgarbage();gone=weak[1]==nil;return a<b end,extra());return gone,t[1],t[2]"#);
}

#[test]
fn finite_pattern_searches_and_backtracking_have_no_invented_work_limit() {
    compare(br#"local s=string.rep('a',400);return string.find(s,'^a*a*a*b'),string.match(s,'^a*a*a*b'),string.gsub(s,'^a*a*a*b','x')"#);
    compare(br#"local next=string.gmatch(string.rep('a',140),'a*a*a*b');return next()"#);
}

#[test]
fn deferred_pattern_errors_remain_reachable_after_long_searches() {
    compare(br#"local s=string.rep('a',500000)..'1';local p=string.rep('a',31)..'%d[';return pcall(string.find,s,p)"#);
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

#[test]
fn extended_math_preserves_bits_and_multiple_result_arity() {
    for source in [
        "return math.asin(0.25),math.acos(0.75),math.atan(1),math.atan2(-0,-1),math.sinh(3),math.cosh(-3),math.tanh(0.5)",
        "return math.deg(0.25),math.rad(-90),math.mod==math.fmod,math.mod(-9,4)",
        "return math.modf(-3.25),math.modf(-3),math.modf(-0.5),math.modf(-1/0)",
        "return math.frexp(0),math.frexp(-0.0),math.frexp(1),math.frexp(-3),math.frexp(1/0)",
        "return math.ldexp(1,-1074),math.ldexp(1.5,-1074),math.ldexp(1.75,-1074),math.ldexp(-1.5,-1075),math.ldexp(1,1024)",
        "local a,b=math.modf(-1/0);local c,d=math.frexp(1/0);return a,b,c,d",
        "local a,b=math.modf(0/0);local c,d=math.frexp(0/0);return a,b,c,d",
        "return math.ldexp('1.5','2'),math.min(0/0,1),math.max(1,0/0),math.log(8,2)",
        "local z=0;z=-z;local a,b=math.modf(z);local c,d=math.frexp(z);return a,b,c,d,math.ldexp(z,2147483647),math.ldexp(1,-2147483648)",
    ] {
        compare(source.as_bytes());
    }
}

#[test]
fn radix_conversions_follow_platform_unsigned_long_width() {
    for source in [
        "return tonumber(' 1010 ',2),tonumber('10',36),tonumber('ffffFFFF',16),tonumber(string.rep('1',42),2)",
        "return tonumber('-ff',16),tonumber('+0xff',16),tonumber('0Xff',16),tonumber('-10000000000000000000000000000',16)",
        "return tonumber('',8),tonumber('  ',9),tonumber('99',8),tonumber('fFfa',15),tonumber('0x',16),tonumber('0x12',36)",
        "return tonumber('12\\0x',16),tonumber('12 \\0x',16),tonumber(123,16),tonumber({},10),tonumber('2',nil)",
        "return tonumber('\\v12\\v',16),tonumber('\\v12\\v'),tonumber('12\\0tail'),tonumber('12 \\0tail'),tonumber('\\0')",
    ] {
        compare(source.as_bytes());
    }
}

#[test]
#[cfg(windows)]
fn seeded_random_matches_the_msvc_reference_stream() {
    for source in [
        "math.randomseed(1);return math.random(),math.random(),math.random(),math.random(3),math.random(-10,10)",
        "math.randomseed(-1);return math.random(),math.random(),math.random(),select('#',math.randomseed(0)),math.random()",
        "math.randomseed(0);local s=0;for i=1,10000 do s=s+math.random(-100,100) end;return s,math.random()",
        "math.randomseed(19);local a=pcall(math.random,0);local b=pcall(math.random,2,1);local c=pcall(math.random,1,2,3);return a,b,c,math.random()",
        "math.randomseed(23);local a=pcall(math.random,'bad');return a,math.random(),math.random(-2147483648,2147483647)",
    ] {
        compare(source.as_bytes());
    }
}

#[test]
fn generated_binary_scaling_rounds_subnormals_once() {
    let mut seed = 0x18f7042cc8f41592u64;
    for _ in 0..2000 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let mut bits = seed;
        if (bits >> 52) & 2047 == 2047 {
            bits ^= 1u64 << 52;
        }
        let value = f64::from_bits(bits);
        let exponent = ((seed >> 17) % 4401) as i32 - 2200;
        let source =
            format!("local a,b=math.frexp({value:e});return a,b,math.ldexp({value:e},{exponent})");
        compare(source.as_bytes());
    }
}

#[test]
fn generated_radix_conversions_include_overflow_and_invalid_digits() {
    let mut seed = 0x51765aa839e05a81u64;
    for _ in 0..2000 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let radix = 2 + (seed >> 6) % 35;
        let len = (seed as usize % 45) + 1;
        let mut bytes = Vec::with_capacity(len + 5);
        bytes.extend_from_slice(if seed & 1 == 0 { b"\x0b-" } else { b"\t+" });
        for i in 0..len {
            let digit = ((seed.rotate_left(i as u32) as u8) % 37) as usize;
            bytes.push(b"0123456789abcdefghijklmnopqrstuvwxyz!"[digit]);
        }
        bytes.extend_from_slice(b"\r\x0b");
        compare(format!("return tonumber({},{radix})", quoted(&bytes)).as_bytes());
    }
}

#[test]
#[cfg(windows)]
fn random_state_is_shared_between_vms_on_the_same_os_thread() {
    let source: &[u8] = b"return math.random()";
    let program = compile(source).unwrap();
    let mut a = Vm::new();
    let mut b = Vm::new();
    a.execute(&compile(b"math.randomseed(123)").unwrap())
        .unwrap();
    let actual = [
        a.execute(&program).unwrap()[0],
        b.execute(&program).unwrap()[0],
        a.execute(&program).unwrap()[0],
    ];
    let a = Lua::new();
    let b = Lua::new();
    a.load("math.randomseed(123)").exec().unwrap();
    let expected = [
        a.load(source).eval::<f64>().unwrap(),
        b.load(source).eval::<f64>().unwrap(),
        a.load(source).eval::<f64>().unwrap(),
    ];
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert_eq!(actual, Value::Number(expected));
    }
}
