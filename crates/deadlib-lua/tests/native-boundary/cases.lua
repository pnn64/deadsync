local sum,top=boundary_sum('17.5',25)
assert(sum==42.5 and top==3)
assert(debug.getinfo(boundary_sum,'u').nups==0)
local payload={tag=42}
local identity,nothing,boolean=boundary_identity(payload)
assert(identity==payload and nothing==nil and boolean==false)
assert(select('#',boundary_identity(payload))==3)
local binary=boundary_binary(function()collectgarbage()end)
assert(#binary==6 and binary==string.char(97,0,98,13,10,255))
assert(boundary_binary()==binary)
local weak=setmetatable({},{__mode='v'})
local held=boundary_call(function()
  local value={tag=99};weak[1]=value;return value
end,nil,function()collectgarbage()end)
assert(held.tag==99 and weak[1]==held)
local many={boundary_stack(function()collectgarbage()end)}
assert(#many==96)
for i=1,96 do assert(many[i]==string.char(i-1,0,256-i))end
local a,b,c,d,e=boundary_mutate(2,3)
assert(a==99 and b==3 and c==nil and d=='7.25' and e==14.5)
assert(select('#',boundary_mutate(2,3))==5)

local inspected=false
debug.sethook(function(event)
  if event=='return'and debug.getinfo(2,'f').func==boundary_sum then
    local _,a=debug.getlocal(2,1);local _,b=debug.getlocal(2,2)
    local _,c=debug.getlocal(2,3);local _,d=debug.getlocal(2,4)
    assert(a=='17.5'and b==25 and c==42.5 and d==3)
    assert(debug.setlocal(2,3,99)=='(*temporary)')
    collectgarbage();inspected=true
  end
end,'r')
local a,b=boundary_sum('17.5',25)
debug.sethook();assert(inspected and a==99 and b==3)

local main_state=boundary_state()
assert(boundary_state()==main_state)
assert(boundary_cached_top(1,2)==2)
assert(boundary_call(function()return boundary_cached_top(1,2,3)end)==3)
assert(boundary_state_reenter(function()
  collectgarbage();return boundary_state()
end))
local function make_thread()
  return coroutine.create(function()
    local state=boundary_state()
    assert(boundary_state_reenter(function()collectgarbage();return boundary_state()end))
    coroutine.yield(state)
    assert(boundary_state()==state)
    return state
  end)
end
local first,second=make_thread(),make_thread()
local ok,a=coroutine.resume(first);assert(ok)
local ok,b=coroutine.resume(second);assert(ok)
assert(a~=b and a~=main_state and b~=main_state)
local ok,value=coroutine.resume(first);assert(ok and value==a)
local ok,value=coroutine.resume(second);assert(ok and value==b)
first=nil;second=nil;collectgarbage()
for i=1,64 do
  local thread=coroutine.create(function()return boundary_state()end)
  assert(coroutine.resume(thread));thread=nil;collectgarbage()
end

local values={false,17,'error\000bytes',payload,function()end,
  coroutine.create(function()end),newproxy(true)}
for i=1,500 do
  local value=values[(i-1)%#values+1]
  local ok,err=pcall(boundary_raise,value)
  assert(not ok and err==value)
end
local ok,err=pcall(boundary_raise,nil)
assert(not ok and err==nil)

local a,b,c,d=boundary_call(function(value)
  collectgarbage()
  assert(value==payload)
  return value,nil,false,43
end,payload)
assert(a==payload and b==nil and c==false and d==43)
assert(select('#',boundary_call(function()end,payload))==0)
assert(select('#',boundary_call(function()return nil,nil,nil end,payload))==3)

for i=1,100 do
  local ok,value=pcall(boundary_call,function(x)
    return boundary_call(function(y)collectgarbage();error(y)end,x)
  end,payload)
  assert(not ok and value==payload)
end
local ok,value=xpcall(function()boundary_raise(payload)end,function(e)
  collectgarbage();return e
end)
assert(not ok and value==payload)
assert(boundary_cached_top(1,2,3,4)==4)

local escaped,inspected_fault,inspected_native
local function fault(value)
  local retained={tag=321}
  escaped=function()return retained end
  error(value,0)
end
local ok,value=xpcall(function()boundary_call(fault,payload)end,function(e)
  for level=2,12 do
    local info=debug.getinfo(level,'f')
    if not info then break end
    if info.func==fault then
      for index=1,10 do
        local name,value=debug.getlocal(level,index)
        if name=='retained' then
          assert(value.tag==321)
          assert(debug.setlocal(level,index,{tag=654})=='retained')
          inspected_fault=true
        end
      end
    elseif info.func==boundary_call then
      local _,value=debug.getlocal(level,2)
      assert(value==payload);inspected_native=true
    end
  end
  assert(inspected_fault and inspected_native)
  collectgarbage();assert(escaped().tag==654)
  return e
end)
assert(not ok and value==payload and escaped().tag==654)

local handled=0
local ok,a,b=xpcall(function()
  return boundary_call(function()
    return load(function()error('reader',0)end)
  end)
end,function(e)handled=handled+1;collectgarbage();return 'outer:'..e end)
assert(ok and a==nil and b=='outer:reader' and handled==1)

local function layered(value)
  local marker=17
  boundary_call(fault,value)
  error('continued after lua_call')
end
local native_frames,found_layer=0,false
local ok,value=xpcall(function()boundary_call(layered,payload)end,function(e)
  for level=2,16 do
    local info=debug.getinfo(level,'f');if not info then break end
    if info.func==boundary_call then native_frames=native_frames+1 end
    if info.func==layered then
      local name,value=debug.getlocal(level,2)
      assert(name=='marker'and value==17);found_layer=true
    end
  end
  collectgarbage();return e
end)
assert(not ok and value==payload and native_frames==2 and found_layer)

local hook_child=function()local marker=42;return marker end
local hook=function(event)
  if event=='call'and debug.getinfo(2,'f').func==hook_child then error(payload,0)end
end
debug.sethook(hook,'c')
local ok,value=xpcall(function()boundary_call(hook_child)end,function(e)
  assert(debug.gethook()==hook);collectgarbage()
  assert(boundary_state()==main_state);return e
end)
debug.sethook();assert(not ok and value==payload)

local ok,value=xpcall(function()
  boundary_call(function()
    local value=newproxy(true);getmetatable(value).__gc=function()error(payload,0)end
    value=nil;collectgarbage()
  end)
end,function(e)collectgarbage();return e end)
assert(not ok and value==payload)

local bad=assert(loadstring('local x=false;return x+1','=native-child'))
local ok,arithmetic=pcall(boundary_call,bad)
local ok2,native=pcall(boundary_call,math.abs,false)
local ok3,callable=pcall(boundary_call,false)
assert(not ok and not ok2 and not ok3)
print('native-errors',arithmetic,native,callable)

local co=coroutine.create(function()
  return boundary_call(function()coroutine.yield()end)
end)
local ok,message=coroutine.resume(co)
assert(not ok and message:find('yield across',1,true))
local ok,message=pcall(boundary_sum,false,2)
assert(not ok and message:find('number expected',1,true))
assert(boundary_sum(40,2)==42)

local weak_closure=setmetatable({},{__mode='v'})
local closure_payload={tag=42};weak_closure[1]=closure_payload
local counter=boundary_make_counter('17.5',closure_payload)
assert(debug.getinfo(counter,'u').nups==2)
assert(select('#',debug.getupvalue(counter,1))==0)
assert(select('#',debug.setupvalue(counter,1,99))==0)
closure_payload=nil;collectgarbage();assert(weak_closure[1]~=nil)
local n,value=counter(24.5);assert(n==42 and value==weak_closure[1])
local other=boundary_make_counter(0,value)
assert(other~=counter and other()==1 and counter()==43)
local closure_thread=coroutine.create(function()return counter(-1)end)
local ok,n,thread_value=coroutine.resume(closure_thread)
assert(ok and n==42 and thread_value==value)
closure_thread=nil;thread_value=nil
value.cycle=counter;weak_closure[2]=counter
counter=nil;other=nil;value=nil;collectgarbage()
assert(weak_closure[1]==nil and weak_closure[2]==nil)

local many_captures=boundary_make_many()
assert(debug.getinfo(many_captures,'u').nups==255)
local a,b,c=many_captures();assert(a==1 and b==255 and c==nil)
assert(select('#',many_captures())==3)
local registry,environment,globals=boundary_snapshot()
assert(registry==debug.getregistry()and environment==_G and globals==_G)
local custom_environment={tag=17}
local snapshot=boundary_set_environment(custom_environment)
assert(debug.getfenv(snapshot)==custom_environment)
local r,e,g=snapshot();assert(r==registry and e==custom_environment and g==globals)
local custom_globals={tag=42}
assert(boundary_set_globals(custom_globals)==globals)
local r,e,g=snapshot();assert(r==registry and e==custom_environment and g==custom_globals)
setfenv(0,globals)
local custom_registry={tag=99}
assert(boundary_registry_roundtrip(custom_registry,function()
  assert(debug.getregistry()==custom_registry);collectgarbage()
end)==custom_registry)
assert(debug.getregistry()==registry)

local invalid_capture=boundary_make_counter(false,{})
local ok,capture_error=pcall(invalid_capture)
local valid_capture=boundary_make_counter(0,{})
local ok2,negative_error=pcall(valid_capture,false)
assert(not ok and not ok2)
print('closure-errors',capture_error,negative_error)
collectgarbage()
print('boundary',sum,top,#binary,'identity','nil','reentry','gc','yield-barrier','recovered')

do
  local payload={tag=42}
  local module,r,e,g=boundary_table(payload)
  assert(module.payload==payload and module[1]==17 and r==debug.getregistry()and e==_G and g==_G)
  collectgarbage();assert(module.snapshot()==debug.getregistry())
  assert(boundary_raw(module,'raw',payload)==payload)
  assert(boundary_raw(module,'raw')==payload)
  assert(boundary_count(module)==4)
  assert(boundary_get(module,'payload')==payload)
  boundary_set(module,'value',99);assert(module.value==99)
  assert(boundary_field(module,17)==17 and boundary_field(module)==17)
  local mt={__metatable='hidden'}
  assert(boundary_metatable(module,mt)==mt and getmetatable(module)=='hidden')
  assert(boundary_metatable(module,nil)==nil)
  local a,b,c,d,f,h,j=boundary_shuffle(payload,payload,17)
  assert(a==payload and b==payload and c==17 and d==3 and f and not h and j==-1)
end

do
  local trace={};local object={};local backing={}
  local mt={
    __index=function(t,k)
      assert(t==object);collectgarbage();trace[#trace+1]='get:'..k
      return backing[k]
    end,
    __newindex=function(t,k,v)
      assert(t==object);collectgarbage();trace[#trace+1]='set:'..k
      backing[k]=v
    end,
  }
  boundary_metatable(object,mt)
  boundary_set(object,'answer',42);assert(boundary_get(object,'answer')==42)
  assert(boundary_field(object,17)==17)
  assert(boundary_raw(object,'answer')==nil)
  assert(table.concat(trace,',')=='set:answer,get:answer,set:field,get:field')
  local proxy=setmetatable({},{__index=object,__newindex=object})
  assert(boundary_get(proxy,'answer')==42)
  boundary_set(proxy,'answer',43);assert(backing.answer==43)
end

do
  local payload={};local object={};local observed=false;local escaped
  local function failing(t,k,v)
    local retained={tag=17};escaped=function()return retained end;error(payload,0)
  end
  setmetatable(object,{__index=failing,__newindex=failing})
  local ok,e=xpcall(function()boundary_set(object,'key',payload)end,function(e)
    local saw_native,saw_lua=false,false
    for level=2,15 do
      local info=debug.getinfo(level,'f');if not info then break end
      if info.func==boundary_set then
        local _,key=debug.getlocal(level,4);local _,value=debug.getlocal(level,5)
        assert(key=='key'and value==payload);saw_native=true
      elseif info.func==failing then
        for index=1,6 do
          local name,value=debug.getlocal(level,index)
          if name=='retained' then
            assert(value.tag==17);debug.setlocal(level,index,{tag=42});saw_lua=true
          end
        end
      end
    end
    collectgarbage();observed=saw_native and saw_lua;return e
  end)
  assert(not ok and e==payload and observed and escaped().tag==42)
  local ok,e=xpcall(function()boundary_field(object)end,function(e)
    local seen=false
    for level=2,15 do
      local info=debug.getinfo(level,'f');if not info then break end
      if info.func==boundary_field then
        assert(debug.getlocal(level,2)==nil);seen=true
      end
    end
    assert(seen);return e
  end)
  assert(not ok and e==payload)
end

do
  local ok,a=pcall(boundary_get,false,'key')
  local ok2,b=pcall(boundary_set,{},nil,42)
  local ok3,c=pcall(boundary_raw,{},0/0,42)
  local ok4,d=pcall(boundary_count,{x=1},'absent')
  assert(not ok and not ok2 and not ok3 and not ok4)
  print('table-errors',a,b,c,d)
  local depth=0
  local object=setmetatable({},{__index=function(t,k)
    depth=depth+1;return boundary_get(t,k)
  end})
  local ok,e=pcall(boundary_get,object,'key')
  assert(not ok and e:find('C stack overflow',1,true))
  print('table-depth',depth)
  local attempts=0;depth=0
  local ok,e=xpcall(function()boundary_get(object,'key')end,function(e)
    attempts=attempts+1
    if attempts<4 then return boundary_get(object,'key')end
    return e
  end)
  assert(not ok)
  print('table-handler-depth',depth,attempts,e)
  local co=coroutine.create(function()
    return boundary_get(setmetatable({},{__index=function()coroutine.yield()end}),'key')
  end)
  local ok,e=coroutine.resume(co)
  assert(not ok and e:find('yield across',1,true))
end


do
  local u,v,distinct=boundary_userdata_empty()
  assert(type(u)=='userdata' and type(v)=='userdata' and distinct)
  assert(boundary_length(u)==0 and boundary_length(v)==0)
  local env={finalized=0};local saved
  local mt={__metatable='hidden',__gc=function(u)
    boundary_userdata_finalize(u);saved=u
  end}
  local u,pointer=boundary_userdata(17,mt)
  assert(getmetatable(u)=='hidden')
  local actual,ok=boundary_environment(u,env);assert(actual==env and ok)
  for i=1,40 do
    local objects={};for j=1,100 do objects[j]=boundary_userdata(j,{})end
    collectgarbage()
    local n,visits,bytes,same=boundary_userdata_read(u,0.5,pointer)
    assert(n==17+i*.5 and visits==i and bytes=='a\0b\r\n\255' and same)
  end
  assert(boundary_length(u)>6 and env.finalized==0)
  u=nil;collectgarbage();collectgarbage()
  assert(saved and env.finalized==1 and env.last_value==37)
  local n,visits,bytes,same=boundary_userdata_read(saved,5,pointer)
  assert(n==42 and visits==41 and same)
  saved=nil;collectgarbage();collectgarbage();assert(env.finalized==1)
  local direct_env={finalized=0}
  do
    local direct=boundary_userdata(73,{__gc=boundary_userdata_finalize})
    boundary_environment(direct,direct_env)
  end
  collectgarbage();collectgarbage()
  assert(direct_env.finalized==1 and direct_env.last_value==73)
  print('userdata',env.finalized,n,visits,direct_env.finalized,direct_env.last_value)
end

do
  local original=boundary_environment(boundary_userdata)
  local env={tag=17}
  assert(select(2,boundary_environment(boundary_userdata,env)))
  local u=boundary_userdata(1)
  assert(boundary_environment(u)==env)
  assert(select(2,boundary_environment(boundary_userdata,original)))
  local function f()return __native_environment_value end
  local env={__native_environment_value=42}
  assert(select(2,boundary_environment(f,env)) and f()==42 and boundary_environment(f)==env)
  local co=coroutine.create(function()return getfenv(0)end)
  local env=setmetatable({thread_value=99},{__index=_G})
  assert(select(2,boundary_environment(co,env)) and boundary_environment(co)==env)
  local ok,actual=coroutine.resume(co);assert(ok and actual==env)
  for _,v in ipairs{false,17,'text',{}}do
    local e,supported=boundary_environment(v,{})
    assert(e==nil and supported==false)
  end
  assert(boundary_environment(nil)==nil and boundary_environment()==nil)
  print('environments',f(),actual.thread_value)
end

do
  local function churn()
    local t={};for i=1,2000 do t[i]=tostring(i)..'\0'..string.rep('x',i%100)end
    collectgarbage()
  end
  for _,value in ipairs{'','a\0b\r\n\255',17,-0,1.5,1e100,1/0,-1/0,0/0}do
    local text,length,kind,stable=boundary_bytes(value,churn)
    assert(text==tostring(value) and length==#text and kind==4 and stable)
    local n,k=boundary_length(value);assert(n==#text and k==4)
    print('bytes',#text,(text:gsub('.',function(c)return string.format('%02x',c:byte())end)))
  end
  for _,v in ipairs{false,{},function()end,coroutine.create(function()end)}do
    local text,length,kind,stable=boundary_bytes(v,churn)
    assert(text==nil and length==0 and kind~=4 and stable)
  end
  assert(boundary_length(setmetatable({1,2,3},{__len=function()error('raw length')end}))==3)
  assert(boundary_length(nil)==0 and boundary_length()==0)
  local f=boundary_capture_bytes(17)
  local text,before,after=f();assert(text=='17' and before==3 and after==4)
  collectgarbage();text,before,after=f();assert(text=='17' and before==4 and after==4)
  assert(select('#',debug.setupvalue(f,1,99))==0)
  text,before,after=f(99);assert(text=='99' and before==3 and after==4)
  print('capture-bytes',text,before,after)
end


do
  local function values(x)return x,nil,false,42 end
  for _,protect in ipairs{boundary_protected,boundary_protected_negative}do
    local status,a,b,c,d=protect(nil,values,-1,17)
    assert(status==0 and a==17 and b==nil and c==false and d==42)
    assert(select('#',protect(nil,values,0))==1)
    assert(select('#',protect(false,values,7))==8)
    local marker={};local escaped;local inspected=false
    local function fail()
      local retained={tag=17};escaped=function()return retained end
      error(marker,0)
    end
    local function handler(e)
      assert(e==marker)
      for level=2,12 do
        local info=debug.getinfo(level,'f');if not info then break end
        if info.func==fail then
          for i=1,6 do
            local name,value=debug.getlocal(level,i)
            if name=='retained' then
              assert(value.tag==17);debug.setlocal(level,i,{tag=42});inspected=true
            end
          end
        end
      end
      collectgarbage();return e
    end
    local status,e=protect(handler,fail,0)
    assert(status==2 and e==marker and inspected and escaped().tag==42)
    collectgarbage();assert(escaped().tag==42)
    local status,e=protect(function()return 'error in error handling'end,fail,-1)
    assert(status==2 and e=='error in error handling')
    local status,e=protect(false,fail,-1)
    assert(status==5 and e=='error in error handling')
    local callable=setmetatable({},{__call=function()error('must not call handler table')end})
    local status,e=protect(callable,fail,-1)
    assert(status==5 and e=='error in error handling')
    local alias_calls=0
    local replacement=function(e)return 'replacement'end
    local function alias_handler(e)
      alias_calls=alias_calls+1
      if alias_calls==1 then
        for level=2,12 do
          local info=debug.getinfo(level,'f');if not info then break end
          if info.func==protect then debug.setlocal(level,1,replacement);break end
        end
        error(e,0)
      end
      return 'snapshot'
    end
    local alias_status,alias_value=protect(alias_handler,fail,-1)
    print('protected-alias',alias_status,alias_value,alias_calls)
    assert(alias_status==2 and alias_value=='replacement' and alias_calls==1)
    local attempts=0
    local status,e=protect(function(e)
      attempts=attempts+1;collectgarbage();if attempts<4 then error(marker,0)end
      return e
    end,fail,-1)
    assert(status==2 and e==marker and attempts==4)
    local calls=0
    local status,a,b=protect(function(e)calls=calls+1;return 'handled'end,function()
      return load(function()error(marker,0)end)
    end,-1)
    assert(status==0 and a==nil and b=='handled' and calls==1)
    local status,a,b=protect(function()error('unexpected outer handler')end,function()
      return protect(nil,fail,-1)
    end,-1)
    assert(status==0 and a==2 and b==marker)
    local co=coroutine.create(function()
      return protect(nil,function()coroutine.yield()end,-1)
    end)
    local ok,status,e=coroutine.resume(co)
    assert(ok and status==2 and e:find('yield across',1,true))
    print('protected',attempts,escaped().tag,select('#',protect(nil,values,7)))
    local overflows=0
    local status,e=protect(function(e)overflows=overflows+1;error(e,0)end,fail,-1)
    assert(status==5 and e=='error in error handling')
    print('protected-overflow',overflows,status,e)
  end
  local ok,balanced=boundary_cpcall(0);assert(ok==0 and balanced)
  local status,e,balanced=boundary_cpcall(1)
  assert(status==2 and e=='cpcall\0\255' and balanced)
  print('cpcall',ok,status,#e,balanced)
end


do
  local function original()return 'original'end
  local replacement=function()return 'argument-replacement'end
  local status,e=boundary_protected_argument(function(handler)
    handler=replacement;collectgarbage();error({},0)
  end,original)
  print('protected-argument',status,e)
  assert(status==2 and e=='argument-replacement')
  local status,e=boundary_protected_argument(function(handler,...)
    handler=replacement;collectgarbage();error({},0)
  end,original)
  print('protected-vararg-argument',status,e)
  assert(status==5 and e=='error in error handling')
end


do
  local status,e=boundary_protected_argument(boundary_replace_handler,function()return 'stale'end)
  print('protected-native-argument',status,e)
  assert(status==2 and e=='native-replacement')
end


do
  local function hex(s)return (s:gsub('.',function(c)return string.format('%02x',c:byte())end))end
  local function emit(label,v)
    assert(v.type==v.after and #v.integer==8)
    print('scalar',label,v.type,v.name,hex(v.number),hex(v.integer),v.isnumber,v.isstring,v.isuserdata,v.iscfunction,v.boolean)
  end
  emit('absent',boundary_scalar());emit('nil',boundary_scalar(nil))
  local ud,pointer=boundary_userdata(1)
  local cases={
    {'false',false},{'true',true},{'zero',0},{'negative-zero',-1/math.huge},
    {'fraction',1.9},{'negative-fraction',-1.9},{'large',2^63},{'minimum',-2^63},
    {'below-limit',2^63-1024},{'above-precision',2^53+1},{'infinity',1/0},{'nan',0/0},
    {'empty',''},{'numeric',' 1.25 '},{'nul-tail','17\0ignored'},{'bad-tail','17x'},
    {'hex','0x10'},{'hex-fraction','0x1.fp2'},{'nan-text','nan'},{'inf-text','inf'},
    {'overflow-text','1e99999'},{'binary','17\255'},{'table',{}},{'userdata',ud},
    {'lightuserdata',pointer},{'native',math.abs},{'module',boundary_scalar},
    {'lua',function()end},{'thread',coroutine.create(function()end)}
  }
  for _,entry in ipairs(cases)do
    emit(entry[1],boundary_scalar(entry[2]))
    local captured=boundary_scalar_capture(entry[2]);emit(entry[1]..'-capture',captured())
  end
  local v=boundary_scalar_null();assert(v.type==2 and v.isuserdata and v.boolean)
  emit('null-lightuserdata',v)
  local rows,absent,truncated=boundary_integers()
  assert(absent==nil and truncated=='a' and #rows==10)
  for i,row in ipairs(rows)do print('integer',i,hex(row.source),hex(row.number),hex(row.integer))end
end

do
  local function compare(a,b,mode)
    local result,balanced=boundary_compare(a,b,mode);assert(balanced);return result
  end
  assert(compare(nil,nil,0) and not boundary_compare(nil,nil,0,1,999))
  assert(not boundary_compare(nil,nil,1,999,1))
  assert(compare(0,-0,0) and not compare(0/0,0/0,0) and not compare(1,'1',0))
  assert(compare(1,2,1) and compare('a\0b','a\0c',1))
  local calls=0
  local function equal(a,b)calls=calls+1;collectgarbage();return {}end
  local a,b=setmetatable({},{__eq=equal}),setmetatable({},{__eq=equal})
  assert(compare(a,b,0) and compare(a,a,0) and calls==1)
  assert(not compare(a,setmetatable({},{__eq=function()return true end}),0))
  local u,v=newproxy(true),newproxy(true)
  getmetatable(u).__eq=equal;getmetatable(v).__eq=equal;assert(compare(u,v,0))
  local callable=setmetatable({},{__call=function()collectgarbage();return 'yes'end})
  setmetatable(a,{__eq=callable,__lt=callable});setmetatable(b,{__eq=callable,__lt=callable})
  assert(compare(a,b,0) and compare(a,b,1))
  local marker={};local retained;local inspected=false
  local function fail(a,b)
    local retained_local={tag=17};retained=function()return retained_local end
    error(marker,0)
  end
  setmetatable(a,{__lt=fail});setmetatable(b,{__lt=fail})
  local ok,e=xpcall(function()return compare(a,b,1)end,function(e)
    for level=2,12 do
      local info=debug.getinfo(level,'f');if not info then break end
      if info.func==fail then
        for i=1,6 do local n,v=debug.getlocal(level,i)
          if n=='retained_local' then debug.setlocal(level,i,{tag=42});inspected=true end
        end
      end
    end
    collectgarbage();return e
  end)
  assert(not ok and e==marker and inspected and retained().tag==42)
  local ok,e=pcall(boundary_compare,1,'1',1)
  assert(not ok and e=='attempt to compare number with string')
  print('comparison',calls,retained().tag,e)
  local depth=0
  local function recurse(a,b)depth=depth+1;return boundary_compare(a,b,0)end
  setmetatable(a,{__eq=recurse});setmetatable(b,{__eq=recurse})
  local ok,e=pcall(boundary_compare,a,b,0)
  assert(not ok and e:find('C stack overflow',1,true));print('comparison-depth',depth)
end

do
  assert(boundary_concat()=='' and select('#',boundary_concat())==1)
  local object={};assert(boundary_concat(object)==object and type(boundary_concat(17))=='number')
  assert(boundary_concat('a\0',17,'',2.5)=='a\000172.5')
  local prefix={};local p,v=boundary_concat_prefix(prefix,17,2.5)
  assert(p==prefix and v=='172.5')
  local p,v=boundary_concat_prefix(prefix);assert(p==prefix and v=='')
  local weak=setmetatable({},{__mode='v'});local observed=false
  local function right()local object=newproxy(true);weak[1]=object;return object end
  local middle=setmetatable({},{__concat=function()return 'middle'end})
  local left;left=setmetatable({},{__concat=function(a,b)
    assert(b=='middle');collectgarbage();collectgarbage();assert(weak[1]~=nil)
    for level=2,12 do
      local info=debug.getinfo(level,'f');if not info then break end
      if info.func==boundary_concat_prefix then
        local _,p=debug.getlocal(level,1);local _,a=debug.getlocal(level,2)
        local _,b=debug.getlocal(level,3)
        assert(p==prefix and a==left and b=='middle')
        local _,last=debug.getlocal(level,4);assert(last==weak[1]);observed=true
      end
    end
    return 'result'
  end})
  local p,v=boundary_concat_prefix(prefix,left,middle,right())
  assert(p==prefix and v=='result' and observed)
  collectgarbage();collectgarbage();assert(weak[1]==nil)
  local a=setmetatable({},{__concat=function()return 42 end})
  assert(type(boundary_concat(a,'x'))=='number' and boundary_concat('prefix',a,'x')=='prefix42')
  local co=coroutine.create(function()
    return boundary_concat(setmetatable({},{__concat=function()coroutine.yield()end}),'x')
  end)
  local ok,e=coroutine.resume(co);assert(not ok and e:find('yield across',1,true))
  local ok,e=pcall(boundary_concat,false,'x');assert(not ok)
  print('concat',observed,v,e)
  local depth=0
  local a=setmetatable({},{__concat=function(a,b)depth=depth+1;return boundary_concat(a,b)end})
  local ok,e=pcall(boundary_concat,a,'x')
  assert(not ok and e:find('C stack overflow',1,true));print('concat-depth',depth)
end


do
  local weak=setmetatable({},{__mode='v'})
  local function equal(a,b)
    for level=2,12 do
      local info=debug.getinfo(level,'f');if not info then break end
      if info.func==boundary_compare then
        debug.setlocal(level,1,nil);debug.setlocal(level,2,nil);break
      end
    end
    a,b=nil,nil;collectgarbage();collectgarbage()
    return weak[1]==nil and weak[2]==nil
  end
  local mt={__eq=equal}
  local function make(i)local value=setmetatable({},mt);weak[i]=value;return value end
  local result,balanced=boundary_compare(make(1),make(2),0)
  print('comparison-roots',result,balanced)
  assert(result and balanced)
end


do
  local function hex(s)return (s:gsub('.',function(c)return string.format('%02x',c:byte())end))end
  local result=boundary_format('a\0ignored',nil,function()
    for i=1,500 do local s=string.rep('x',i)..i end;collectgarbage()
  end)
  assert(result.pointer and result.stable)
  print('format',hex(result.main),hex(result.va),result.unknown,result.pointer,result.stable)
  assert(result.unknown=='literal % / %q %02d')
  local result=boundary_format(nil);print('format-null',hex(result.main))
  local oldpause=collectgarbage('setpause',0)
  local text,uncollected=boundary_format_gc()
  collectgarbage('setpause',oldpause)
  assert(text=='parts:message:17:2.5' and uncollected)
  print('format-gc',text,uncollected)
  assert(boundary_auxwhere(0)=='' and boundary_auxwhere(-1)=='' and boundary_auxwhere(999)=='')
  print('where',boundary_auxwhere(1))
  local named=assert(loadstring('return function()return boundary_auxwhere(1)end','=aux-name'))()
  assert(named()=='aux-name:1: ')
  print('where-named',named())
  local tail;tail=function(n)if n==0 then return boundary_auxwhere(2)end return tail(n-1)end
  assert(tail(3)=='')
end

do
  local text,length,kind=boundary_auxcheck(17,0)
  assert(text=='17' and length==2 and kind==4)
  local text,length,kind=boundary_auxcheck('a\0b',0)
  assert(text=='a\0b' and length==3 and kind==4)
  local text,length=boundary_auxcheck(nil,1);assert(text=='fallback' and length==8)
  local text,length=boundary_auxcheck(nil,2);assert(text==nil and length==0)
  local bytes,n,kind=boundary_auxcheck('7.9',3);assert(n==7 and #bytes==8 and kind==4)
  assert(select(2,boundary_auxcheck(nil,4))==77 and boundary_auxcheck(nil,5)==2.5)
  local object={};assert(boundary_auxcheck(object,6)==object and boundary_auxcheck(nil,7)==nil)
  local option,kind=boundary_auxcheck(1,8);assert(option==2 and kind==4)
  assert(boundary_auxcheck(nil,8)==1 and boundary_auxcheck('first\0ignored',8)==0)
  assert(boundary_auxcheck(10,9))
  for _,case in ipairs{{false,0},{false,3},{17,6},{'bad',8},{8001,9},{false,10},{false,11},{false,12}}do
    local ok,e=pcall(boundary_auxcheck,case[1],case[2]);assert(not ok)
    print('aux-error',case[2],e)
  end
  local ok,e=pcall(boundary_auxcheck);assert(not ok);print('aux-missing',e)
  local ok,e=pcall(function()return boundary_auxerror('error\0tail')end)
  assert(not ok);print('aux-located-error',e)
  local holder={named=boundary_auxcheck}
  local ok,e=pcall(function()return holder:named(10)end)
  assert(not ok);print('aux-method-error',e)
  local aliases={['nul\0tail']=boundary_auxcheck}
  local ok,e=pcall(function()return aliases['nul\0tail'](false,0)end)
  assert(not ok);print('aux-nul-name',#e,(e:gsub('%z','<NUL>')))
end

do
  local created,mt=boundary_auxnewmetatable('deadlua.aux.test')
  local again,same=boundary_auxnewmetatable('deadlua.aux.test')
  assert(created and not again and mt==same)
  debug.getregistry()['deadlua.aux.false']=false
  local created,value=boundary_auxnewmetatable('deadlua.aux.false')
  assert(not created and value==false)
  local object,pointer=boundary_auxuserdata(17)
  assert(object:get()==17 and object:set('42')==object and object:get()==42)
  assert(boundary_auxget(object,-2)==42)
  local tag=debug.getmetatable(object)
  debug.setmetatable(pointer,tag);assert(boundary_auxget(pointer)==42)
  debug.setmetatable(pointer,nil);assert(object:get()==42)
  local wrong=newproxy(true)
  for _,entry in ipairs{{false,1},{wrong,1},{wrong,-2}}do
    local ok,e=pcall(boundary_auxget,entry[1],entry[2]);assert(not ok)
    print('aux-userdata-error',entry[2],e)
  end
  local ok,e=pcall(function()return tag.get(false)end);assert(not ok)
  print('aux-userdata-method-error',e)
  local meta=setmetatable({},{__index=function()error('raw metafield lookup')end})
  meta.value=false;meta.action=setmetatable({},{__call=function(_,obj)collectgarbage();return obj end})
  local object=setmetatable({},meta)
  local present,value=boundary_auxmetafield(object,'value',false);assert(present and value==false)
  assert(boundary_auxmetafield(object,'missing',false)==false)
  local present,value=boundary_auxmetafield(object,'action',true);assert(present and value==object)
  local ok,e=pcall(boundary_auxmetafield,object,'value',true);assert(not ok)
  print('aux-metafield-error',e)
end

do
  local first,second={},{}
  local original=boundary_environment(boundary_auxopen)
  boundary_environment(boundary_auxopen,{_private_tag=42})
  local module=boundary_auxopen('deadlua_aux.nested',first,second)
  boundary_environment(boundary_auxopen,original)
  assert(deadlua_aux.nested==module and package.loaded['deadlua_aux.nested']==module)
  local a,b,tag=module.echo();assert(a==first and b==second and tag==42)
  assert(boundary_environment(module.echo)._private_tag==42)
  collectgarbage();a,b=module.echo();assert(a==first and b==second)
  local weak=setmetatable({first,second},{__mode='v'});first=nil;second=nil;a=nil;b=nil
  package.loaded['deadlua_aux.nested']=nil;deadlua_aux=nil;module=nil
  collectgarbage();collectgarbage();assert(weak[1]==nil and weak[2]==nil)
  local table={};assert(boundary_auxopen(nil,table)==table)
  assert(table.echo() == nil and type(table.checked)=='function')
  local cached={};package.loaded['deadlua_aux.cached']=cached
  _G.deadlua_aux=false;assert(boundary_auxopen('deadlua_aux.cached')==cached)
  _G.deadlua_aux=nil;package.loaded['deadlua_aux.cached']=nil
  _G.deadlua_aux={blocked=false}
  local ok,e=pcall(boundary_auxopen,'deadlua_aux.blocked.tail');assert(not ok)
  print('aux-open-error',e);_G.deadlua_aux=nil
  local root={};local found,conflict=boundary_auxfind(root,'a..b.')
  assert(conflict==nil and found==root.a[''].b[''])
  root.a.bad=false
  assert(boundary_auxfind(root,'a.bad.tail')=='bad.tail')
  print('aux-registration',tag,type(table.echo),conflict==nil)
end

do
  local references={};local object={};local weak=setmetatable({object},{__mode='v'})
  local first=boundary_auxrefs(references,object);assert(first==1 and references[first]==object)
  object=nil;collectgarbage();assert(weak[1]==references[first])
  assert(boundary_auxrefs(references,nil)==-1)
  local second=boundary_auxrefs(references,{});assert(second==2)
  assert(boundary_auxrefs(references,first,1));collectgarbage();collectgarbage();assert(weak[1]==nil)
  local reused=boundary_auxrefs(references,42);assert(reused==first and references[reused]==42)
  assert(boundary_auxrefs(references,-1,1))
  print('aux-references',first,second,reused)
end


do
  local observed,detail=0,false
  local ok,e=xpcall(function()boundary_auxcheck(false,0)end,function(e)
    for level=2,12 do
      local info=debug.getinfo(level,'f');if not info then break end
      if info.func==boundary_auxcheck then
        for i=1,12 do
          local name,value=debug.getlocal(level,i);if not name then break end
          observed=i;if i==3 then detail=value=='string expected, got boolean'end
        end
        break
      end
    end
    return e
  end)
  assert(not ok and detail and observed==3)
  print('aux-error-stack',observed,detail)
end

do
  local capacity,bytes,offset=boundary_auxbuffer_info()
  assert(capacity>0 and bytes>=offset+capacity)
  print('aux-buffer-abi',capacity,bytes,offset)
  local checks=0
  local function check(steps,callback)
    local expected={}
    for i,step in ipairs(steps)do
      local mode,value=step[1],step[2]
      if mode~=5 and mode~=6 then
        if type(value)=='number'then value=tostring(value)end
        if type(value)~='string'then value=''end
        if mode==1 then value=value:match('^[^%z]*')end
        expected[#expected+1]=value
      end
    end
    local result,trace=boundary_auxbuffer(steps,callback)
    assert(result==table.concat(expected) and #trace==#steps)
    local observation={}
    for i,row in ipairs(trace)do
      assert(row[1]==row[3] and row[2]>=0 and row[2]<=capacity)
      observation[#observation+1]=table.concat(row,',')
    end
    checks=checks+1
    print('aux-buffer',checks,#result,table.concat(observation,';'))
    return result
  end
  check({});check({{5},{5},{6},{5},{6}})
  local sizes={0,1,capacity-1,capacity,capacity+1,2*capacity-1,2*capacity,2*capacity+1,17*capacity+13}
  for mode=0,4 do
    for _,size in ipairs(sizes)do
      local text=('a\0bc'):rep(math.ceil(size/4)):sub(1,size)
      check({{mode,text}})
      check({{0,'prefix'},{mode,text},{0,'suffix'},{5},{5}})
    end
  end
  -- Exact-full buffers defer their flush until the next nonempty append.
  check({{0,('x'):rep(capacity)},{0,''},{2,''},{1,''},{3,'y'},{6},{2,123.5}})
  check({{2,nil},{2,false},{2,true},{2,{}},{2,function()end},{2,17},{2,-0.125}})
  local lengths={capacity*64,capacity*32,capacity*16,capacity*8,capacity*4,capacity*2,capacity,capacity,capacity,capacity,capacity,capacity,capacity}
  local descending,ascending={},{}
  for i,length in ipairs(lengths)do
    descending[i]={2,('d'):rep(length)}
    ascending[#lengths-i+1]={2,('u'):rep(length)}
  end
  check(descending);check(ascending)
  local seed,mixed=17,{}
  for i=1,400 do
    seed=(seed*25173+13849)%65536
    mixed[i]={seed%7,('m\0'):rep(math.floor((seed%(3*capacity+1))/2))}
  end
  check(mixed)
  local callbacks=0
  local function collect()
    callbacks=callbacks+1
    collectgarbage()
    assert(boundary_sum(1,2)==3)
    local nested=boundary_auxbuffer({{0,'inner\0'},{2,17},{4,'tail'}})
    assert(nested=='inner\0'..'17tail')
  end
  local steps={}
  for i=1,40 do steps[i]={i%5,('z\0'):rep(capacity+i)}end
  check(steps,collect);assert(callbacks==#steps)
  local marker={}
  local ok,e=pcall(boundary_auxbuffer,{{0,('x'):rep(capacity+1)}},function()error(marker)end)
  assert(not ok and e==marker)
  check({{2,'after error'},{4,'\0recovered'}})
  local co=coroutine.create(function()
    return pcall(boundary_auxbuffer,{{2,'retained'}},function()coroutine.yield()end)
  end)
  local resumed,success,message=coroutine.resume(co)
  assert(resumed and not success and message:find('yield',1,true))
  print('aux-buffer-yield',message)
  for _,row in ipairs({
    {'abc','x','r','abc'}, {'','x','r',''}, {'aaaaa','aa','X','XXa'},
    {'a+b+a+','a+','R','Rb+R'}, {'x\0x','x','a\0b','a'},
    {'aba','a\0ignored','R','RbR'}, {'aaa','a','',''},
    {123412,'12',9,'9349'}, {('ab'):rep(2000),'b',('r'):rep(17),('a'..('r'):rep(17)):rep(2000)},
  })do
    local result,same=boundary_auxgsub(row[1],row[2],row[3])
    assert(result==row[4] and same)
    print('aux-gsub',#result,same)
  end
  print('aux-buffer-complete',checks,callbacks)
end

do
  local function read(f,n,callback,index)
    local name,value,delta=boundary_upget(f,n,callback,index)
    assert(delta==(name==nil and 0 or 1))
    return name,value
  end
  local function write(f,n,value,index)
    local name,delta=boundary_upset(f,n,value,index)
    assert(delta==(name==nil and 0 or -1))
    return name
  end
  local a,b={mark=17},29
  local function get()return a,b end
  local original=a
  for _,n in ipairs({-2147483648,-1,0,1,2,3,256,2147483647})do
    local name,value=read(get,n)
    assert(name==(n==1 and 'a' or n==2 and 'b' or nil))
    if n==1 then assert(value==a)elseif n==2 then assert(value==b)else assert(value==nil)end
    print('upvalue-index',n,name,type(value))
  end
  local collected=0
  local name,value=read(get,1,function()
    collected=collected+1
    collectgarbage()
    assert(read(get,2)=='b')
    boundary_format('%s','nested')
  end)
  assert(name=='a' and value==original and collected==1)
  local replacement={mark=42}
  assert(write(get,1,replacement)=='a' and a==replacement)
  assert(write(get,2,51,-5)=='b' and b==51)
  assert(read(get,1,nil,-4)=='a')
  assert(write(get,3,false)==nil and b==51)
  assert(write(false,1,17)==nil and read(false,1)==nil)
  assert(read(get,1,nil,5)==nil)
  local bound_read,bound_write=boundary_upbind(get)
  assert(read(bound_read,1)=='')
  assert(bound_read(false,1,nil,-10003)=='a')
  assert(bound_write(false,2,61,-10003)=='b' and b==61)
  assert(select('#',debug.getupvalue(bound_read,1))==0)
  local counter=boundary_make_counter(10)
  local cname,cvalue=read(counter,1);assert(cname=='' and cvalue==10)
  assert(write(counter,1,40)=='' and counter()==41)
  local many=boundary_make_many(255)
  assert(read(many,255)=='' and write(many,255,99)=='')
  assert(select(2,read(many,255))==99 and read(many,256)==nil)

  local captured
  local co=coroutine.create(function()
    local cell=17
    captured=function()return cell end
    coroutine.yield()
    assert(cell==71)
    return cell
  end)
  assert(coroutine.resume(co))
  assert(write(captured,1,71)=='cell')
  local ok,result=coroutine.resume(co);assert(ok and result==71)
  assert(write(captured,1,81)=='cell' and captured()==81)

  local pname,iterator=read(pairs,1);assert(pname=='' and type(iterator)=='function')
  assert(iterator==select(1,pairs({})) and iterator~=next)
  local function custom(t,k)return next(t,k)end
  assert(write(pairs,1,custom)=='')
  local count=0;for k,v in pairs({x=17,y=29})do count=count+v end
  assert(count==46 and select(1,pairs({}))==custom)
  assert(write(pairs,1,iterator)=='')
  local _,istep=read(ipairs,1)
  assert(write(ipairs,1,custom)=='' and select(1,ipairs({}))==custom)
  assert(write(ipairs,1,istep)=='')

  local _,valid=read(newproxy,1)
  assert(type(valid)=='table' and getmetatable(valid)==valid and valid.__mode=='kv')
  local proxy=newproxy(true);local mt=getmetatable(proxy)
  assert(valid[mt]==true)
  valid[mt]=false;assert(not pcall(newproxy,proxy))
  valid[mt]=17;assert(getmetatable(newproxy(proxy))==mt)
  local alternate={}
  assert(write(newproxy,1,alternate)=='')
  assert(not pcall(newproxy,proxy))
  alternate[mt]=true;assert(getmetatable(newproxy(proxy))==mt)
  local other=newproxy(true);assert(alternate[getmetatable(other)]==true)
  assert(write(newproxy,1,valid)=='')

  local matches=string.gmatch('ab12cd34','%d+')
  assert(select(2,read(matches,1))=='ab12cd34')
  assert(select(2,read(matches,2))=='%d+')
  assert(select(2,read(matches,3))==0 and matches()=='12')
  assert(select(2,read(matches,3))==4)
  assert(write(matches,3,'6')=='' and matches()=='34' and matches()==nil)
  assert(write(matches,1,12345)=='' and write(matches,2,2)=='' and write(matches,3,0)=='')
  assert(matches()=='2' and select(2,read(matches,1))=='12345' and select(2,read(matches,2))=='2')

  local file=assert(io.tmpfile());file:write('first\nsecond\n');file:seek('set',0)
  local lines=file:lines()
  assert(select(2,read(lines,1))==file and select(2,read(lines,2))==false)
  assert(lines()=='first')
  local otherfile=assert(io.tmpfile());otherfile:write('replacement\n');otherfile:seek('set',0)
  assert(write(lines,1,otherfile)=='' and write(lines,2,17)=='')
  assert(lines()=='replacement' and lines()==nil and io.type(otherfile)=='closed file')
  file:close()
  local wrapper=coroutine.wrap(function()return 17 end)
  local _,thread=read(wrapper,1);assert(type(thread)=='thread')
  local otherthread=coroutine.create(function()return 91 end)
  assert(write(wrapper,1,otherthread)=='' and wrapper()==91)

  for count=0,2 do
    for _,style in ipairs({'original','empty','nul'})do
      local first,second,third=assert(loadfile('upvalues-'..count..'-'..style..'.luac'))()
      for n=1,2 do
        local name,value=read(first,n,function()collectgarbage()end)
        local expected=n<=count and (style=='original' and (n==1 and 'a' or 'b')or style=='empty' and ''or 'name')or nil
        assert(name==expected)
        assert(value==(n<=count and (n==1 and 17 or 29)or nil))
        assert(write(first,n,100+n)==expected)
      end
      local x,y=second();local p,q=third()
      assert(x==(count>=1 and 101 or 17) and y==(count>=2 and 102 or 29)and x==p and y==q)
      print('upvalue-native',count,style,x,y)
    end
  end
  print('native-upvalues-complete')
end

do
  local function gc(what,data,...)
    local result,delta,after=boundary_gc(what,data,...)
    assert(delta==0 and after==what)
    return result
  end
  assert(boundary_gc_counts())
  assert(select(2,boundary_gc_counts()))
  assert(gc(0)==0)
  for _,what in ipairs({-2147483648,-1,8,2147483647})do assert(gc(what,17)==-1)end
  local pause=gc(6,200)
  local stepmul=gc(7,200)
  for _,control in ipairs({6,7})do
    local previous=200
    for _,value in ipairs({-2147483648,-17,0,1,173,2147483647,200})do
      assert(gc(control,value)==previous)
      previous=value
    end
  end
  assert(gc(1)==0)
  assert(gc(2)==0)
  assert(gc(7,0)==200)
  assert(gc(5,0)==1)
  assert(gc(7,200)==0)
  for _,size in ipairs({-1,2147483647})do assert(gc(5,size)==1)end
  print('native-gc-controls',true,true)

  gc(0)
  local sentinel={}
  local observed,locals=false,0
  do
    local proxy=newproxy(true)
    getmetatable(proxy).__gc=function()
      assert(debug.getinfo(2,'f').func==boundary_gc)
      for i=1,8 do
        local name,value=debug.getlocal(2,i)
        if not name then break end
        locals=i
        if i==3 then observed=value==sentinel end
      end
      assert(locals==3 and observed)
      assert(debug.setlocal(2,1,7))
      assert(boundary_sum(17,25)==42)
      gc(2)
    end
  end
  local result,delta,changed=boundary_gc(2,0,sentinel)
  assert(result==0 and delta==0 and changed==7 and observed and locals==3)
  print('native-gc-finalizer',locals,observed,changed)

  gc(0)
  gc(7,0)
  local stepped=false
  do local p=newproxy(true);getmetatable(p).__gc=function()
    assert(debug.getinfo(2,'f').func==boundary_gc)
    assert(select(2,debug.getlocal(2,1))==5)
    stepped=true
  end end
  assert(gc(5,0)==1 and stepped)
  gc(7,200)
  print('native-gc-step',stepped)

  gc(0)
  local marker={}
  local detail=false
  do
    local proxy=newproxy(true)
    getmetatable(proxy).__gc=function()
      local retained=marker
      gc(0)
      error(retained)
    end
  end
  local ok,e=xpcall(function()boundary_gc(2,0,sentinel)end,function(e)
    for level=2,16 do
      local info=debug.getinfo(level,'f');if not info then break end
      if info.func==boundary_gc then
        local name,value=debug.getlocal(level,3)
        detail=name~=nil and value==sentinel
        break
      end
    end
    return e
  end)
  assert(not ok and e==marker and detail)
  local finalized=false
  do local p=newproxy(true);getmetatable(p).__gc=function()finalized=true end end
  for i=1,2000 do local pressure={i,i+1}end
  assert(not finalized)
  assert(gc(2)==0 and finalized)
  print('native-gc-error',e==marker,detail,finalized)

  gc(0)
  local yielded=false
  do
    local p=newproxy(true)
    getmetatable(p).__gc=function()
      local co=coroutine.create(function()coroutine.yield(42);return 17 end)
      local ok,value=coroutine.resume(co);assert(ok and value==42)
      ok,value=coroutine.resume(co);assert(ok and value==17)
      yielded=true
    end
  end
  assert(gc(2)==0 and yielded)
  gc(0)
  do
    local p=newproxy(true)
    getmetatable(p).__gc=function()coroutine.yield()end
  end
  local co=coroutine.create(function()return pcall(boundary_gc,2)end)
  local resumed,ok,e=coroutine.resume(co)
  assert(resumed and not ok and e:find('yield',1,true))
  print('native-gc-yield',yielded,e)
  gc(2)
  gc(6,pause);gc(7,stepmul)
  print('native-gc-complete')
end

do
  local checked=0
  local chunk_file=assert(io.open('upvalues-2-original.luac','rb'))
  local native_chunk=chunk_file:read('*a');assert(chunk_file:close())
  local function buffer(source,name,mode)
    local status,value,delta=boundary_loadbuffer(mode or 0,source,name)
    assert(delta==1)
    return status,value
  end
  local function reader(parts,name,visit)
    local status,value,delta,calls,last=boundary_load(function(i)
      if visit then visit(i)end
      return parts[i]
    end,name)
    assert(delta==1)
    return status,value,calls,last
  end
  local sources={
    'return 17+25', 'local a=17;return function(b)return a+b end',
    '--[=[comment]=]\nreturn "a\\000z"', 'return [==[a\nb]==]',
    'return 1 .. "z"', 'return -0', 'local =', 'return "unfinished',
    'return 0x12g', '\27Lua', 'return 1\0', '',
    native_chunk
  }
  for case,source in ipairs(sources)do
    local expected,message=buffer(source,'=native chunk')
    for split=1,#source+1 do
      local parts={source:sub(1,split),source:sub(split+1)}
      local status,value,calls,last=reader(parts,'=native chunk')
      assert(status==expected)
      if status~=0 then assert(value==message)end
      if status==0 and case==1 then assert(value()==42)end
      if status==0 and case==2 then assert(value()(25)==42)end
      if status==0 and case==13 then local g=value();local a,b=g();assert(a==17 and b==29)end
      checked=checked+1
      print('native-load-split',case,split,status,calls)
    end
    local parts={};for i=1,#source do parts[i]=source:sub(i,i)end
    local status,value,calls=reader(parts,'=native chunk')
    assert(status==expected and (status==0 or value==message))
    print('native-load-bytes',case,status,calls)
  end
  for _,name in ipairs{'=named','@dir/chunk.lua','long\0ignored','',string.rep('x',100)}do
    local status,value=buffer('local =',name)
    local s,v=reader({'local ='},name)
    assert(status==3 and s==3 and v==value)
    print('native-load-name',value)
  end
  local status,f=reader({false,'return 42'})
  assert(status==0 and f()==42)
  status,f=reader({'','return 17'})
  assert(status==0 and f()==17)
  status,f=buffer('return 42\0invalid','ignored',1)
  assert(status==0 and f()==42)
  status,f=buffer('local =','ignored',1)
  assert(status==3);print('native-load-cstring',f)
  status,f=buffer('return 42\0invalid')
  assert(status==0 and f()==42);print('native-load-null',f())

  local seen=0
  status,f=reader({'return ','42'},'=reentry',function(i)
    assert(debug.getinfo(3,'f').func==boundary_load)
    collectgarbage()
    local s,g=buffer('return 17');assert(s==0 and g()==17)
    local co=coroutine.create(function()coroutine.yield(29)end)
    local ok,value=coroutine.resume(co);assert(ok and value==29)
    seen=seen+1
  end)
  assert(status==0 and f()==42 and seen==3)
  local marker,replacement={},{}
  local function failure()error(marker)end
  local ok,s,e=xpcall(function()return reader({1},'=runtime',function()failure()end)end,
    function(value)assert(value==marker);return replacement end)
  assert(ok and s==2 and e==replacement)
  local used=false
  ok,s,e=xpcall(function()return reader({'local ='})end,function()used=true end)
  assert(ok and s==3 and not used)
  ok,s,e=xpcall(function()return reader({},'=handler',function()failure()end)end,
    function()error('handler failed')end)
  assert(ok and s==5 and e=='error in error handling')
  print('native-load-errors',ok,s,e)
  local co=coroutine.create(function()
    local old=getfenv(0)
    local env=setmetatable({answer=42},{__index=old})
    local status,f=reader({'return answer'},'=environment',function()setfenv(0,env)end)
    setfenv(0,old)
    assert(status==0 and getfenv(f)==env and f()==42)
  end)
  assert(coroutine.resume(co))
  -- Parser depth contributes to nested native calls made by a reader.
  for _,depth in ipairs{0,1,10,100,190,195,196,197,198,199,200}do
    local source='return '..string.rep('(',depth)..'42'..string.rep(')',depth)
    local status,value,calls=reader({source:sub(1,-2),source:sub(-1)},'=depth')
    print('native-load-depth',depth,status,calls,status==0 and value()or value)
  end
  for _,outer in ipairs{0,10,100,190}do
    local parts={'return '..string.rep('(',outer)..'42',string.rep(')',outer)..' '}
    for _,inner in ipairs{0,5,90,190,195,196,197}do
      local status,value,calls=reader(parts,'=outer',function(i)
        local source='return '..string.rep('(',inner)..'17'..string.rep(')',inner)
        local s,v=buffer(source,'=inner')
        print('native-load-reader-depth',outer,inner,i,s,s==0 and v()or v)
      end)
      assert(status==0 and value()==42)
    end
  end
  print('native-load-complete',checked)
end

do
  local function load(path)
    local status,value,delta=boundary_loadfile(path)
    assert(delta==1)
    return status,value
  end
  local function write(path,bytes)
    local f=assert(io.open(path,'wb'));assert(f:write(bytes));assert(f:close())
  end
  local original=loadfile
  loadfile=function()error('native loading must not use _G.loadfile')end
  local path='native-file.lua'
  local cases={
    '', 'return 42', '#!/lua\nreturn 42', '# no newline',
    '#!\r\nlocal =', 'local =', '--comment\r\nreturn 42',
    '#!'..string.rep('header',2000)..'\nreturn 42',
    string.rep('--comment\n',1000)..'return 42',
    '\239\187\191return 42', 'return 42\26invalid',
  }
  local binary=assert(io.open('upvalues-2-original.luac','rb'))
  local bytes=binary:read('*a');assert(binary:close())
  cases[#cases+1]=bytes
  cases[#cases+1]='#!/lua\n'..bytes
  cases[#cases+1]='#!'..string.rep('long',3000)..'\r\n'..bytes
  cases[#cases+1]=bytes:sub(1,-2)
  cases[#cases+1]='#!/lua\n\27Lua'
  for i,source in ipairs(cases)do
    write(path,source)
    local status,value=load(path..'\0ignored')
    local reference,message=original(path)
    assert((status==0)==(reference~=nil))
    if status~=0 then assert(status==3 and value==message)end
    if status==0 and i==2 then assert(value()==42)end
    if status==0 and i>=12 then
      local g=value();local a,b=g();assert(a==17 and b==29)
    end
    assert(os.remove(path)) -- named streams must already be closed on Windows
    print('native-file',i,status,status~=0 and value or 'loaded')
  end
  local status,value=load('native-file-missing.lua')
  assert(status==6);print('native-file-open',status,value)
  status,value=load('')
  assert(status==6);print('native-file-empty-path',status,value)
  local handled=false
  local ok,s,e=xpcall(function()return load('native-file-missing.lua')end,
    function()handled=true end)
  assert(ok and s==6 and not handled)
  write(path,'local =')
  ok,s,e=xpcall(function()return load(path)end,function()handled=true end)
  assert(ok and s==3 and not handled);assert(os.remove(path))

  -- luaL_loadfile(NULL) shares the actual stdin FILE/lookahead, not io.input.
  write(path,'return 99')
  local alternate=assert(io.open(path,'r'));io.input(alternate)
  assert(io.stdin:read('*n')==17)
  status,value=load(nil);assert(status==0 and value()==42)
  assert(io.stdin:read('*a')=='')
  status,value=load(nil);assert(status==0 and value()==nil)
  assert(io.type(io.stdin)=='file' and io.read('*a')=='return 99')
  io.input(io.stdin);assert(alternate:close());assert(os.remove(path))
  local co=coroutine.create(function()
    write(path,'return answer')
    local old=getfenv(0);local env=setmetatable({answer=73},{__index=old})
    setfenv(0,env)
    local status,f=load(path)
    setfenv(0,old)
    assert(status==0 and getfenv(f)==env and f()==73)
    assert(os.remove(path))
  end)
  assert(coroutine.resume(co))
  loadfile=original
  print('native-file-complete',#cases)
end

do
  local function hex(s)return(s:gsub('.',function(c)return string.format('%02x',c:byte())end))end
  local function dump(f,writer,stop,status)
    local s,pieces,trace,delta,argument,calls=boundary_dump(f,writer,stop,status)
    assert(delta==0 and #pieces==calls and #trace==calls)
    return s,table.concat(pieces),trace,argument,calls
  end
  local names={}
  for count=0,2 do for _,style in ipairs{'original','empty','nul'}do
    names[#names+1]='upvalues-'..count..'-'..style..'.luac'
  end end
  names[#names+1]='dump-constants.luac';names[#names+1]='dump-stripped.luac'
  local canonical,canonical_bytes,canonical_trace
  for _,name in ipairs(names)do
    local f=assert(loadfile(name))
    local status,bytes,trace,argument,calls=dump(f)
    assert(status==0 and argument==f and bytes==string.dump(f))
    assert(trace[1]==12)
    print('native-dump-chunk',name,calls,table.concat(trace,','),hex(bytes))
    if name=='dump-constants.luac'then canonical=f;canonical_bytes=bytes;canonical_trace=trace end
    if name=='dump-stripped.luac'then
      local zeros=0
      local s,b=dump(f,function(i,block,null)if #block==0 then assert(null);zeros=zeros+1 end end)
      assert(s==0 and b==bytes and zeros==4)
    end
  end
  local length=0
  for stop=1,#canonical_trace do
    length=length+canonical_trace[stop]
    local status,bytes,trace,argument,calls=dump(canonical,nil,stop,17)
    assert(status==17 and calls==stop and argument==canonical)
    assert(bytes==canonical_bytes:sub(1,length))
    print('native-dump-stop',stop,calls,#bytes,status)
  end
  for _,status in ipairs{-2147483648,-1,1,2147483647}do
    local s,bytes,trace,argument,calls=dump(canonical,nil,2,status)
    assert(s==status and calls==2);print('native-dump-status',s)
  end
  local calls=0
  local status,bytes,trace,argument=dump(canonical,function(i,block)
    assert(debug.getinfo(2,'f').func==boundary_dump)
    collectgarbage()
    local co=coroutine.create(function()coroutine.yield(42)end)
    local ok,value=coroutine.resume(co);assert(ok and value==42)
    local s,f=boundary_loadbuffer(0,'return 17');assert(s==0 and f()==17)
    assert(debug.setlocal(2,1,91));calls=calls+1
  end)
  assert(status==0 and argument==91 and bytes==canonical_bytes and calls==#trace)
  local marker={};local retained=false
  local ok,e=xpcall(function()return dump(canonical,function()error(marker)end)end,function(e)
    for level=2,15 do
      local info=debug.getinfo(level,'f');if not info then break end
      if info.func==boundary_dump then retained=select(2,debug.getlocal(level,1))==canonical end
    end
    return e
  end)
  assert(not ok and e==marker and retained)
  for _,value in ipairs{false,17,'bytes',{},boundary_sum,newproxy(),coroutine.create(function()end)}do
    local s,b,t,a,c=dump(value);assert(s==1 and b=='' and c==0 and a==value)
    assert(boundary_dump_null(value)==1)
  end
  assert(select(1,dump(nil))==1 and boundary_dump_null(nil)==1)
  local f=function(a,...)return a,select('#',...)end
  local s,b=dump(f);assert(s==0)
  local g=assert(loadstring(b));local a,n=g(42,1,nil);assert(a==42 and n==2)
  local saved=string.dump;string.dump=function()error('must not use string.dump')end
  s,b=dump(f);string.dump=saved;assert(s==0 and assert(loadstring(b))(17)==17)
  print('native-dump-complete',#names,#canonical_trace)
end

do
  local main,is_main,current,delta,status=boundary_thread()
  assert(type(main)=='thread' and is_main and current and delta==1 and status==0)
  local main_pointer,same=boundary_thread_pointer(main)
  assert(same and main_pointer==boundary_state() and boundary_thread_status(main)==0)
  local co=coroutine.create(function()
    local self,is_main,current,delta,status=boundary_thread()
    assert(self==coroutine.running() and not is_main and current and delta==1 and status==0)
    assert(boundary_thread_status(main)==0 and coroutine.status(main)=='normal')
    local pointer,same=boundary_thread_pointer(self);assert(same and pointer==boundary_state())
    coroutine.yield(pointer)
    assert(boundary_thread_status(self)==0)
    return pointer
  end)
  -- Resolve a C state before this coroutine has ever called native code.
  local pointer,same=boundary_thread_pointer(co)
  assert(not same and pointer~=main_pointer and boundary_thread_status(co)==0)
  local captured=boundary_thread_capture(co)
  local p,s=captured();assert(p==pointer and s==0)
  collectgarbage();assert(boundary_thread_pointer(co)==pointer)
  local ok,p=coroutine.resume(co);assert(ok and p==pointer and boundary_thread_status(co)==1)
  local p,s=captured();assert(p==pointer and s==1)
  print('native-thread-yield',boundary_thread_status(co),boundary_thread_status(main))
  ok,p=coroutine.resume(co);assert(ok and p==pointer and boundary_thread_status(co)==0)
  assert(not coroutine.resume(co));assert(boundary_thread_status(co)==0)
  local marker={}
  local failed=coroutine.create(function()assert(select(5,boundary_thread())==0);error(marker)end)
  local failed_pointer=boundary_thread_pointer(failed)
  ok,p=coroutine.resume(failed);assert(not ok and p==marker and boundary_thread_status(failed)==2)
  assert(not coroutine.resume(failed));assert(boundary_thread_status(failed)==2)
  assert(boundary_thread_pointer(failed)==failed_pointer)
  print('native-thread-failure',boundary_thread_status(failed),boundary_thread_status(co))
  local protected=coroutine.create(function()assert(not pcall(error,marker));return 42 end)
  assert(coroutine.resume(protected));assert(boundary_thread_status(protected)==0)
  local weak=setmetatable({},{__mode='v'})
  do local fresh=coroutine.create(function()error('never resumed')end)
    weak[1]=fresh;boundary_thread_pointer(fresh)
  end
  collectgarbage();assert(weak[1]==nil)
  local retained
  do local fresh=coroutine.create(function()end)
    weak[2]=fresh;retained=boundary_thread_capture(fresh);assert(select(2,retained())==0)
  end
  collectgarbage();assert(weak[2]~=nil)
  retained=nil;collectgarbage();assert(weak[2]==nil)
  local all,pointers={},{}
  for i=1,100 do
    all[i]=coroutine.create(function()end)
    local p=boundary_thread_pointer(all[i]);assert(not pointers[p]);pointers[p]=true
  end
  collectgarbage()
  for i=1,#all do assert(pointers[boundary_thread_pointer(all[i])])end
  assert(boundary_thread_pointer(co,-2)==pointer)
  for _,v in ipairs{false,17,'thread',{},boundary_sum,newproxy()}do
    assert(boundary_thread_status(v)==nil)
    local p,same=boundary_thread_pointer(v);assert(not same)
    assert(p==select(1,boundary_thread_pointer(nil)))
  end
  assert(boundary_thread_status(nil)==nil)
  print('native-thread-complete',#all)
end

do
  local function pointer(value,visit)
    local p,stable,delta,formatted,compatible=boundary_pointer(value,visit)
    assert(stable and delta==0 and compatible)
    return p,formatted
  end
  local null=pointer(nil)
  for _,value in ipairs{false,true,0,17,'bytes'}do assert(pointer(value)==null)end
  local objects={{},{},function()end,function()end,print,ipairs,newproxy(),newproxy(),coroutine.create(function()end)}
  local pointers={}
  for _,value in ipairs(objects)do
    local p,formatted=pointer(value,function()collectgarbage()end)
    assert(p~=null and not pointers[p]);pointers[p]=true
    assert(tostring(value)==type(value)..': '..formatted)
    assert(pointer(p)==p and tostring(p)=='userdata: '..formatted)
  end
  local main=boundary_thread()
  assert(pointer(main)==boundary_state())
  assert(pointer(main)==boundary_thread_pointer(main))
  local co=objects[#objects]
  assert(pointer(co)==boundary_thread_pointer(co))
  local ok=coroutine.resume(co);assert(ok)
  assert(pointer(co)==boundary_thread_pointer(co))
  local table_value={};local p=pointer(table_value)
  setmetatable(table_value,{__tostring=function()return 'custom' end})
  assert(tostring(table_value)=='custom' and pointer(table_value)==p)
  local equal=0
  local mt={__eq=function()equal=equal+1;return true end}
  local a,b=setmetatable({},mt),setmetatable({},mt)
  assert(a==b and pointer(a)~=pointer(b) and equal==1)
  local weak=setmetatable({},{__mode='v'})
  do local t={};weak[1]=t;local opaque=pointer(t);assert(type(opaque)=='userdata')end
  collectgarbage();assert(weak[1]==nil)
  local live={}
  for i=1,1000 do live[i]={};local p=pointer(live[i]);assert(not pointers[p]);pointers[p]=true end
  collectgarbage();for i=1,#live do assert(pointers[pointer(live[i])])end
  local p,formatted=pointer(null);assert(p==null and tostring(null)=='userdata: '..formatted)
  print('native-pointer-complete',#objects,#live)
end
