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
