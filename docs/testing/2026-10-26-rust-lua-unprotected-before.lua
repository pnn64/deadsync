local sum,top=boundary_sum('17.5',25)
assert(sum==42.5 and top==3)
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

local co=coroutine.create(function()
  return boundary_call(function()coroutine.yield()end)
end)
local ok,message=coroutine.resume(co)
assert(not ok and message:find('yield across',1,true))
local ok,message=pcall(boundary_sum,false,2)
assert(not ok and message:find('number expected',1,true))
assert(boundary_sum(40,2)==42)
collectgarbage()
print('boundary',sum,top,#binary,'identity','nil','reentry','gc','yield-barrier','recovered')
