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
