local call=boundary_thread_pcall
local total=0
local function check(label,co,fn,wanted,handler,...)
  local previous=coroutine.status(co)
  local status,values,before,after,old,new=call(co,fn,wanted,handler,...)
  assert(old==new and coroutine.status(co)==previous)
  local trace={}
  for i=1,after do
    local value=values[i]
    trace[i]=type(value)=='number' and tostring(value) or type(value)
  end
  print(label,status,before,after,old,new,table.concat(trace,','))
  total=total+1
  return status,values,before,after
end
local function make(kind)
  local co
  if kind=='fresh' then co=coroutine.create(function()return 17 end)
  elseif kind=='completed' then co=coroutine.create(function()end);assert(coroutine.resume(co))
  elseif kind=='yielded' then co=coroutine.create(function()coroutine.yield(17,29);return 57 end);assert(coroutine.resume(co))
  elseif kind=='yield-tail' then co=coroutine.create(function(...)return boundary_thread_yield_tail(...)end);assert(coroutine.resume(co,17,29,57))
  elseif kind=='failed' then co=coroutine.create(function()error({tag='old'})end);assert(not coroutine.resume(co))
  end
  return co
end
for _,kind in ipairs{'fresh','completed','yielded','yield-tail','failed'} do
  local co=make(kind)
  local s,v,b,a=check(kind..'-return',co,function(...)return ... end,-1,nil,31,37)
  assert(s==0 and v[a-1]==31 and v[a]==37)
  if kind=='fresh' then local ok,value=coroutine.resume(co);assert(ok and value==17)
  elseif kind=='yielded' then local ok,value=coroutine.resume(co,41);assert(ok and value==57)
  elseif kind=='yield-tail' then local ok,value=coroutine.resume(co,41);assert(ok and value==41)
  else assert(coroutine.status(co)=='dead')end
  local token={tag='error'}
  co=make(kind);s,v,b,a=check(kind..'-error',co,function(e)error(e)end,-1,nil,token)
  assert(s==2 and v[a]==token)
  co=make(kind);s,v,b,a=check(kind..'-handler',co,function(e)error(e)end,3,function(e)assert(coroutine.running()==co);collectgarbage();return e end,token)
  assert(s==2 and v[a]==token)
  for wanted=0,3 do
    co=make(kind);s,v,b,a=check(kind..'-fixed-'..wanted,co,function()return 31,37 end,wanted,nil)
    local delta=kind=='yield-tail' and 2 or 0
    assert(s==0 and a==b+delta+wanted)
    if wanted>=1 then assert(v[b+delta+1]==31)end
    if wanted>=2 then assert(v[b+delta+2]==37)end
    if wanted>=3 then assert(v[a]==nil)end
  end
  co=make(kind);s,v,b,a=check(kind..'-C',co,boundary_thread_called,2,nil)
  assert(s==0 and v[a-1]==co and v[a]==(kind=='failed' and 2 or (kind=='yielded' or kind=='yield-tail') and 1 or 0))
  co=make(kind);s,v,b,a=check(kind..'-handler-failure',co,function(e)error(e)end,0,function()error('handler failed')end,token)
  assert(s==5 and v[a]=='error in error handling')
  co=make(kind);s,v,b,a=check(kind..'-nonfunction-handler',co,function(e)error(e)end,0,{},token)
  assert(s==5 and v[a]=='error in error handling')
  for _,mode in ipairs{false,true}do
    co=make(kind)
    local status,before,after,old,error=boundary_thread_cpcall(co,mode)
    assert(status==(mode and 2 or 0))
    assert(after==before+(kind=='yield-tail' and 2 or 0)+(mode and 1 or 0))
    assert(error==(mode and debug.getfenv(co) or nil))
    print(kind..'-cpcall',mode,status,before,after,old,type(error))
    total=total+1
  end
  if boundary_level then
    co=make(kind);s,v,b,a=check(kind..'-call-level',co,function()return boundary_level(coroutine.running())end,1,nil)
    assert(s==0 and type(v[a])=='number')
  end
end
if boundary_level then
  local source=make('fresh');local co=make('fresh')
  local s,v,b,a=check('level-write-success',co,function()
    boundary_setlevel(source,co);return boundary_level(co)
  end,1,nil)
  assert(s==0 and v[a]==0 and boundary_level(co)==65535)
  print('level-after-success',boundary_level(co))
  co=make('fresh');local token={}
  s,v,b,a=check('level-write-error',co,function()
    boundary_setlevel(source,co);error(token)
  end,1,nil)
  assert(s==2 and v[a]==token and boundary_level(co)==0)
  print('level-after-error',boundary_level(co))
end
-- Error handlers see the real target frames and close the mutated cells on failure.
do
  local co=make('fresh');local saved,seen
  local function fail(e)local live='original';saved=function()return live end;error(e)end
  local function handler(e)
    for level=1,12 do
      if not debug.getinfo(level)then break end
      for i=1,40 do
        local name=debug.getlocal(level,i);if not name then break end
        if name=='live' then assert(debug.setlocal(level,i,'changed')=='live');seen=true end
      end
    end
    assert(seen and coroutine.running()==co);return e
  end
  local token={};local s,v,b,a=check('handler-locals',co,fail,1,handler,token)
  assert(s==2 and v[a]==token and saved()=='changed')
end
-- A callback can call back through its original waiting thread, sharing one heap.
do
  local worker=coroutine.create(function()
    local original=coroutine.running();local target=make('fresh')
    local function callback()
      assert(coroutine.running()==target)
      local s,v,b,a=check('reenter-owner',original,function()
        assert(coroutine.running()==original);collectgarbage();return 41
      end,1,nil)
      assert(s==0 and v[a]==41);return 42
    end
    local s,v,b,a=check('reenter-target',target,callback,1,nil)
    assert(s==0 and v[a]==42 and coroutine.running()==original)
  end)
  assert(coroutine.resume(worker))
end
-- The target's ordinary hooks run without an added visible call activation.
do
  local co=make('fresh');local events={}
  local function hook(event)
    if event=='call' or event=='return' then
      local info=debug.getinfo(2,'S');events[#events+1]=event..':'..info.what
    end
  end
  debug.sethook(co,hook,'cr')
  local s,v,b,a=check('hooks',co,function(x)return x+1 end,1,nil,41)
  debug.sethook(co)
  assert(s==0 and v[a]==42)
  print('hook-events',table.concat(events,','))
end
do
  local co=make('fresh')
  local s,v,b,a=check('yield-rejected',co,function()coroutine.yield(17)end,-1,nil)
  assert(s==2 and v[a]=='attempt to yield across metamethod/C-call boundary')
end
print('thread-calls-complete',total)
