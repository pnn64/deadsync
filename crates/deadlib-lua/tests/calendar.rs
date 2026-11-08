use deadlib_lua::{Value, Vm, compile};
use lua_reference::{Lua, MultiValue, Value as Ref};
use std::sync::Mutex;

// PUC os.date reads libc's shared gmtime/localtime buffer while building its
// result. Independent oracle states must not execute those reads concurrently.
static CALENDAR: Mutex<()> = Mutex::new(());

#[derive(Debug, PartialEq)]
enum Scalar {
    Nil,
    Boolean(bool),
    Number(u64),
    Bytes(Vec<u8>),
}
fn compare(source: &[u8]) {
    let _calendar = CALENDAR.lock().unwrap();
    let mut vm = Vm::new();
    let program = deadlib_lua::compile_named(source, b"=calendar-test").unwrap();
    let actual: Vec<_> = vm
        .execute(&program)
        .unwrap()
        .to_vec()
        .into_iter()
        .map(|v| match v {
            Value::Nil => Scalar::Nil,
            Value::Boolean(b) => Scalar::Boolean(b),
            Value::Number(n) => Scalar::Number(n.to_bits()),
            Value::String(id) => Scalar::Bytes(vm.string_bytes(id).unwrap().to_vec()),
            v => panic!("scalar observation: {v:?}"),
        })
        .collect();
    // SAFETY: fixed local calendar probes need the development oracle's debug
    // library; they load no third-party native code. Use a plain protected entry.
    let lua = unsafe { Lua::unsafe_new() };
    let values: MultiValue = lua.exec_raw_lua(|raw| unsafe {
        use lua_reference::{FromLuaMulti, ffi};
        let state = raw.state();
        let start = ffi::lua_gettop(state);
        let mut status = ffi::luaL_loadbuffer(
            state,
            source.as_ptr().cast(),
            source.len(),
            c"=calendar-test".as_ptr(),
        );
        if status == 0 {
            status = ffi::lua_pcall(state, 0, ffi::LUA_MULTRET, 0);
        }
        if status != 0 {
            let mut len = 0;
            let bytes = ffi::lua_tolstring(state, -1, &mut len);
            let message = if bytes.is_null() {
                "non-string error".into()
            } else {
                String::from_utf8_lossy(std::slice::from_raw_parts(bytes.cast(), len)).into_owned()
            };
            panic!("reference: {message}");
        }
        assert_ne!(ffi::lua_checkstack(state, 32), 0);
        let values = MultiValue::from_stack_multi(ffi::lua_gettop(state) - start, raw).unwrap();
        ffi::lua_settop(state, start);
        values
    });
    let expected: Vec<_> = values
        .into_iter()
        .map(|v| match v {
            Ref::Nil => Scalar::Nil,
            Ref::Boolean(b) => Scalar::Boolean(b),
            Ref::Number(n) => Scalar::Number(n.to_bits()),
            Ref::Integer(n) => Scalar::Number((n as f64).to_bits()),
            Ref::String(s) => Scalar::Bytes(s.as_bytes().to_vec()),
            v => panic!("reference scalar: {v:?}"),
        })
        .collect();
    assert_eq!(actual.len(), expected.len());
    for (index, (a, b)) in actual.iter().zip(&expected).enumerate() {
        if let (Scalar::Bytes(a), Scalar::Bytes(b)) = (a, b) {
            assert_eq!(
                String::from_utf8_lossy(a),
                String::from_utf8_lossy(b),
                "observation {index}"
            );
        }
        assert_eq!(a, b, "observation {index}");
    }
}

#[test]
fn fixed_dates_preserve_utc_local_fields_and_native_format_bytes() {
    compare(br#"
      local out={};local times={-43201,-43200,-3601,-1,-.9,0,.9,1,3600,86399,86400,
        951782400,2147483647,2147483648,32535215999,32535216000,32536771200,
        9007199254740992,9223372036854775808,-9223372036854775808,0/0,math.huge,-math.huge,'0',' 86400 ','0x10'}
      for _,n in ipairs(times) do for _,prefix in ipairs({'','!'}) do
        local t=os.date(prefix..'*t',n);out[#out+1]=t and 'table' or 'nil'
        if t then
          for _,key in ipairs({'sec','min','hour','day','month','year','wday','yday','isdst'})do out[#out+1]=tostring(t[key])end
          out[#out+1]=os.date(prefix..'%a|%A|%b|%B|%c|%d|%H|%I|%j|%m|%M|%p|%S|%U|%w|%W|%x|%X|%y|%Y|%z|%Z|%%',n)
        end
      end end
      return table.concat(out,';')
    "#);
}

#[test]
fn time_fields_default_coerce_normalize_and_leave_the_input_table_unchanged() {
    compare(br#"
      local out={};local fields={'sec','min','hour','day','month','year','isdst'}
      local cases={{},{sec='12.9',min='03',hour='0x10'},
        {year=2000,month=2,day=30,hour=25,min=-1,sec=60},
        {sec=false,min={},hour='bad'},{year=1969,month=12,day=31,hour=23},
        {year=3001},{year=2147483647},{month=-12,day=0},
        {sec=0/0},{hour=math.huge},{isdst=true},{isdst=false},{isdst=0},{isdst=''}}
      for _,changes in ipairs(cases) do
        local t={year=2000,month=1,day=1};for k,v in pairs(changes)do t[k]=v end
        local before={};for i,k in ipairs(fields)do before[i]=t[k]end
        local ok,n=pcall(os.time,t);out[#out+1]=tostring(ok);out[#out+1]=tostring(n)
        for i,k in ipairs(fields)do local a,b=before[i],t[k];out[#out+1]=tostring(a==b or (type(a)=='number' and a~=a and b~=b))end
      end
      return table.concat(out,'|')
    "#);
}

#[test]
fn calendar_argument_validation_and_missing_fields_keep_complete_diagnostics() {
    compare(br#"
      local out={};local function save(f,...)local ok,e=pcall(f,...);out[#out+1]=tostring(ok);out[#out+1]=tostring(e)end
      save(os.date,{},{});save(os.date,false,{});save(os.date,'',{});save(os.date,nil,false)
      save(os.time,false);save(os.time,'2000');save(os.time,1);save(os.time,{})
      save(os.time,{day=true});save(os.time,{day=1});save(os.time,{day=1,month='bad'})
      save(os.time,{day=1,month=1});save(os.time,{day=1,month=1,year=false})
      local t={date=os.date,time=os.time};save(function()return t:date()end);save(function()return t:time()end)
      return table.concat(out,'|')
    "#);
}

#[test]
fn time_field_lookups_run_in_order_and_observe_mutations_and_nested_calls() {
    compare(br#"
      local trace={};local t={}
      setmetatable(t,{__index=function(self,key)
        trace[#trace+1]=key;collectgarbage()
        if key=='sec' then self.min='7';return '3.9' end
        if key=='hour' then return 12 end
        if key=='day' then self.month=2;return 30 end
        if key=='year' then return 2000 end
        if key=='isdst' then assert(type(os.time{year=2001,month=1,day=1})=='number');return false end
      end})
      local n=os.time(t,'ignored',false);return n,table.concat(trace,','),t.min,t.month,t.year==nil
    "#);
}

#[test]
fn time_lookup_errors_keep_identity_and_reach_xpcall_before_unwind() {
    compare(br#"
      local out={};local object={};local trace={}
      local t=setmetatable({},{__index=function(_,key)trace[#trace+1]=key;if key=='hour'then error(object,0)end end})
      local ok,e=pcall(os.time,t);out[#out+1]=tostring(ok);out[#out+1]=tostring(e==object);out[#out+1]=table.concat(trace,',')
      trace={};local ok,e=xpcall(function()return os.time(t)end,function(e)
        local found=false;for i=1,8 do local info=debug.getinfo(i,'f');if info and info.func==os.time then found=true end end
        return e==object and found
      end)
      out[#out+1]=tostring(ok);out[#out+1]=tostring(e)
      for _,bad in ipairs({'day','month','year'})do
        trace={};local t=setmetatable({},{__index=function(_,key)
          trace[#trace+1]=key;if key==bad then return false end
          if key=='day' or key=='month' then return 1 elseif key=='year'then return 2000 end
        end})
        local ok,e=pcall(os.time,t);out[#out+1]=tostring(ok);out[#out+1]=e;out[#out+1]=table.concat(trace,',')
      end
      return table.concat(out,'|')
    "#);
}

#[test]
fn time_lookup_callbacks_preserve_native_yield_barriers() {
    compare(
        br#"
      local out={};local t=setmetatable({},{__index=function()return coroutine.yield('bad')end})
      local c=coroutine.create(function()return os.time(t)end);local ok,e=coroutine.resume(c)
      out[#out+1]=tostring(ok);out[#out+1]=e;out[#out+1]=coroutine.status(c)
      local trace={};local t=setmetatable({},{__index=function(_,key)
        local ok,e=pcall(coroutine.yield);trace[#trace+1]=tostring(ok)..':'..e
        if key=='year'then return 2000 elseif key=='day' or key=='month'then return 1 end
      end})
      local c=coroutine.create(function()return os.time(t)end);local ok,n=coroutine.resume(c)
      out[#out+1]=tostring(ok);out[#out+1]=tostring(n);out[#out+1]=table.concat(trace,',')
      return table.concat(out,'|')
    "#,
    );
}

#[test]
fn date_literals_nul_boundaries_numeric_formats_and_single_percent_match_lua51() {
    compare(
        br#"
      return os.date('',0),os.date('!',0),os.date('%',0),os.date('!%%:%Y:%',0),
        os.date('a\0%Q',0),os.date('!*t\0ignored',0).year,os.date(123.5,0),
        os.date(nil,0),os.date('%Q',math.huge),os.date('!!%Y',0)
    "#,
    );
}

#[test]
fn time_current_readings_are_inside_the_official_second_brackets() {
    let lua = Lua::new();
    let reference = lua.load("return os.time()").into_function().unwrap();
    let mut vm = Vm::new();
    let program = compile(b"return os.time(nil,false),os.time()").unwrap();
    for _ in 0..128 {
        let before: f64 = reference.call(()).unwrap();
        let values = vm.execute(&program).unwrap();
        let after: f64 = reference.call(()).unwrap();
        for value in values {
            let Value::Number(actual) = value else {
                panic!("time result")
            };
            assert!(before <= *actual && *actual <= after);
        }
    }
    compare(br#"local before=os.time();local t=os.date('*t');local n=os.time(t);local after=os.time();return before<=n and n<=after,type(os.date()),type(os.date(nil))"#);
}

#[test]
fn time_discards_extra_arguments_before_field_callbacks_collect() {
    compare(br#"
      local finalized=0;local function proxy()local p=newproxy(true);getmetatable(p).__gc=function()finalized=finalized+1 end;return p end
      local seen={};local t=setmetatable({},{__index=function(_,key)
        collectgarbage();seen[#seen+1]=finalized
        if key=='year'then return 2000 elseif key=='day' or key=='month'then return 1 end
      end})
      local n=os.time(t,proxy());return n,table.concat(seen,','),finalized
    "#);
}

#[test]
fn discarded_time_argument_slots_do_not_drop_caller_locals_or_open_upvalues() {
    compare(br#"
      local finalized=0;local p=newproxy(true);getmetatable(p).__gc=function()finalized=finalized+1 end
      local hold=function()return p end;local weak=setmetatable({p},{__mode='v'});local seen={}
      local t=setmetatable({},{__index=function(_,key)
        collectgarbage();seen[#seen+1]=tostring(finalized)..':'..tostring(weak[1]==hold())
        if key=='year'then return 2000 elseif key=='day' or key=='month'then return 1 end
      end})
      local n=os.time(t,p,hold());return n,type(p),hold()==p,table.concat(seen,','),finalized
    "#);
}

#[test]
fn completed_time_field_values_are_popped_before_later_callbacks_and_return_hooks() {
    compare(br#"
      local finalized=0;local trace={}
      local function proxy()local p=newproxy(true);getmetatable(p).__gc=function()finalized=finalized+1 end;return p end
      local t=setmetatable({},{__index=function(_,key)
        if key=='sec' or key=='hour' or key=='isdst'then return proxy()end
        collectgarbage();trace[#trace+1]=key..':'..finalized
        if key=='day' or key=='month'then return 1 elseif key=='year'then return 2000 end
      end})
      debug.sethook(function(event)
        if event=='return' and debug.getinfo(2,'f').func==os.time then
          collectgarbage();trace[#trace+1]='return:'..finalized
        end
      end,'r')
      local n=os.time(t);debug.sethook();return n,table.concat(trace,','),finalized
    "#);
}

#[test]
fn recursive_time_lookups_reach_the_reference_native_call_limit_and_recover() {
    compare(
        br#"
      local t={};local calls=0
      setmetatable(t,{__index=function()calls=calls+1;return os.time(t)end})
      local ok,e=pcall(os.time,t)
      local recovered=os.time{year=2000,month=1,day=1}
      return ok,calls,e,type(recovered)
    "#,
    );
}

#[test]
fn generated_calendar_normalization_and_round_trips_match_the_reference() {
    compare(br#"
      local seed=517;local function rnd()seed=(seed*25173+13849)%65536;return seed end
      local out={}
      for i=1,512 do
        local t={year=1900+rnd()%1110,month=1+rnd()%16,day=1+rnd()%40,hour=rnd()%30,min=rnd()%70,sec=rnd()%70}
        if i%3==1 then t.isdst=false elseif i%3==2 then t.isdst=true end
        local n=os.time(t);out[#out+1]=tostring(n)
        if n then
          out[#out+1]=os.date('!%Y-%m-%d %H:%M:%S|%j|%w',n)
          local date=os.date('*t',n);out[#out+1]=tostring(os.time(date)==n)
        end
      end
      return table.concat(out,';')
    "#);
}

#[test]
fn date_result_tables_preserve_mutation_traversal_and_collection_after_hash_growth() {
    compare(
        br#"
      local out={};local weak=setmetatable({},{__mode='v'})
      for _,prefix in ipairs({'','!'})do
        local d=os.date(prefix..'*t',951782400);weak[1]=d
        local original={};for key,value in pairs(d)do original[key]=value end
        local keys={};local key=next(d)
        while key~=nil do keys[#keys+1]=key;d[key]=nil;key=next(d,key)end
        table.sort(keys);out[#out+1]=table.concat(keys,',')
        out[#out+1]=tostring(next(d)==nil and #d==0)
        for key,value in pairs(original)do d[key]=value end
        for i=1,257 do d['extra'..i]=i;d[i]=i*2 end
        d.self=d;d[original]=original
        setmetatable(d,{__index=function(_,key)return key end})
        collectgarbage();local sum=0;local count=0
        for i=1,257 do sum=sum+d['extra'..i]+d[i]end
        for _ in pairs(d)do count=count+1 end
        out[#out+1]=tostring(sum);out[#out+1]=tostring(count)
        out[#out+1]=tostring(d.self==d and d[original]==original and weak[1]==d)
        out[#out+1]=tostring(os.time(d)==os.time(original));out[#out+1]=d.missing
        d=nil;collectgarbage();out[#out+1]=tostring(weak[1]==nil)
      end
      return table.concat(out,'|')
    "#,
    );
}
