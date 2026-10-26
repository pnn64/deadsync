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
