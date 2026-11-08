local call=boundary_thread_pcall
local count=0
local function make(kind)
  local co=coroutine.create(function()
    if kind=='failed' then error('old') end
    if kind=='yielded' then coroutine.yield() end
  end)
  if kind~='fresh' then coroutine.resume(co) end
  return co
end
local function trace(label,status,values,before,after,old,new)
  local types={}
  for i=1,after do
    local value=values[i]
    types[i]=type(value)=='number' and tostring(value)or type(value)
  end
  print(label,status,before,after,old,new,table.concat(types,','))
  count=count+1
end
for _,kind in ipairs{'fresh','completed','yielded','failed'}do
  for _,wanted in ipairs{-1,0,1,3,5}do
    for _,native in ipairs{false,true}do
      local co=make(kind)
      local fn=native and boundary_thread_yield_tail or function(v)return coroutine.yield(v)end
      local s,v,b,a,old,new=call(co,fn,wanted,nil,17)
      if kind=='fresh' then
        assert(s==2 and new==0 and v[a]=='attempt to yield across metamethod/C-call boundary')
      else
        assert(s==0 and new==1 and a==1 and v[1]==17 and coroutine.status(co)=='suspended')
      end
      trace(kind..'-'..wanted..'-'..tostring(native),s,v,b,a,old,new)
      collectgarbage()
    end
  end
  local co=make(kind)
  local s,b,a,new,value=boundary_thread_cpcall(co,2)
  assert(s==(kind=='fresh' and 2 or 0) and new==(kind=='fresh' and 0 or 1))
  print(kind..'-cpcall-yield',s,b,a,new,type(value));count=count+1
end
-- The pending Lua frame owns its locals after the API has returned.
for _,wanted in ipairs{-1,0,1,3,5}do
  local co=make('completed')
  local weak=setmetatable({},{__mode='v'})
  local fn=function()
    local held={value=29};weak[1]=held
    local a,b=coroutine.yield(17)
    return held.value,a,b
  end
  local s,v,b,a,old,new=call(co,fn,wanted,nil)
  assert(s==0 and new==1 and a==1 and v[1]==17)
  collectgarbage();collectgarbage();assert(weak[1] and weak[1].value==29)
  local function pack(...)return select('#',...),{...}end
  local n,r=pack(coroutine.resume(co,41,43))
  assert(r[1] and coroutine.status(co)=='dead')
  assert(n==1+(wanted==-1 and 3 or wanted))
  if wanted==-1 or wanted>=1 then assert(r[2]==29)else assert(r[2]==nil)end
  if wanted==-1 or wanted>=2 then assert(r[3]==41)else assert(r[3]==nil)end
  if wanted==-1 or wanted>=3 then assert(r[4]==43)else assert(r[4]==nil)end
  fn=nil;collectgarbage();collectgarbage();assert(weak[1]==nil)
  print('resume',wanted,n,r[2],r[3],r[4]);count=count+1
end
do
  local co=make('completed');local exported
  local weak=setmetatable({},{__mode='v'})
  local fn=function()
    local held={value=29};weak[1]=held
    exported=function()return held.value end
    coroutine.yield(17);return held.value
  end
  local s,v,b,a,old,new=call(co,fn,-1,nil)
  assert(s==0 and new==1 and exported()==29)
  assert(debug.setupvalue(exported,1,{value=37})=='held')
  collectgarbage();collectgarbage();assert(weak[1]==nil and exported()==37)
  local ok,value=coroutine.resume(co);assert(ok and value==37 and exported()==37)
  print('open-cell',ok,value,exported());count=count+1
end
-- The original error handler has expired when the yielded computation resumes.
do
  local co=make('completed');local calls=0;local token={}
  local handler=function(e)calls=calls+1;return e end
  local fn=function()coroutine.yield(17);error(token)end
  local s,v,b,a,old,new=call(co,fn,-1,handler)
  assert(s==0 and new==1 and calls==0)
  local ok,error=coroutine.resume(co);assert(not ok and error==token and calls==0)
  print('resumed-error',ok,error==token,calls);count=count+1
end
print('api-yields-complete',count)
