local count=0
local function make(kind)
  local co=coroutine.create(function()
    if kind=='failed' then error('old')end
    if kind=='yielded' then coroutine.yield(17)end
    if kind=='partial' then return boundary_thread_yield_tail(11,13,17)end
    return 23
  end)
  if kind~='fresh' then coroutine.resume(co)end
  return co
end
for _,kind in ipairs{'fresh','completed','yielded','failed','partial'}do
  for _,source in ipairs{'return 29','return function(' ,string.dump(assert(loadstring('return 31','=thread-binary')))}do
    local co=make(kind);local visits=0
    local s,b,a,old,new,n,result=boundary_thread_load(co,source,function(i,status)
      visits=visits+1;assert(i==visits and coroutine.running()==co)
      assert(status==((kind=='yielded' or kind=='partial')and 1 or kind=='failed' and 2 or 0))
      collectgarbage()
    end,'=thread-load',3)
    assert(old==new and n==visits)
    if source=='return function(' then assert(s==3 and type(result)=='string')
    else assert(s==0 and type(result)=='function' and result()==(source=='return 29'and 29 or 31))end
    print(kind,s,b,a,old,new,n,type(result));count=count+1
    if kind=='fresh' then local ok,v=coroutine.resume(co);assert(ok and v==23)
    elseif kind=='yielded' then local ok,v=coroutine.resume(co);assert(ok and v==23)
    elseif kind=='partial' and s==0 then local ok,v=coroutine.resume(co,41);assert(ok and v==41)end
  end
  do
    local co=make(kind);local token={}
    local s,b,a,old,new,n,result=boundary_thread_load(co,'return 1',function()error(token)end,'=reader-error',1)
    assert(s==2 and result==token and old==new and n==1)
    print(kind..'-reader-error',s,b,a,old,new,n,result==token);count=count+1
  end
  do
    local co=make(kind)
    local env={marker=37};debug.setfenv(co,env)
    local s,b,a,old,new,n,result=boundary_thread_load(co,'return marker',nil,'=target-environment',2)
    assert(s==0 and getfenv(result)==env and result()==37 and old==new)
    print(kind..'-environment',s,b,a,old,new,n,result());count=count+1
  end
  for _,entry in ipairs{{},{name='=explicit\000ignored'},{name='@relative-file'}}do
    local co=make(kind)
    local s,b,a,old,new,n,result=boundary_thread_load(co,'return function(',nil,entry.name,2)
    assert(s==3 and type(result)=='string'and old==new)
    print(kind..'-name',s,b,a,old,new,n,result);count=count+1
  end
end
do
  local parent=boundary_thread_called()
  local child=coroutine.create(function()
    for _,fail in ipairs{false,true}do
      local token={};local visits=0
      local s,b,a,old,new,n,result=boundary_thread_load(parent,'return 43',function()
        visits=visits+1;assert(coroutine.running()==nil)
        collectgarbage();if fail then error(token)end
      end,'=waiting-parent',2)
      assert(old==0 and new==0 and n==visits)
      assert(fail and s==2 and result==token or not fail and s==0 and result()==43)
      print('waiting-parent',fail,s,b,a,old,new,n,type(result));count=count+1
    end
  end)
  local ok,e=coroutine.resume(child);assert(ok,e)
end
do
  local outer=make('completed');local inner=make('completed');local visits=0
  local s,b,a,old,new,n,result=boundary_thread_load(outer,'return 47',function()
    if visits==0 then
      visits=visits+1
      local s,b,a,old,new,n,result=boundary_thread_load(inner,'return 53',function()
        assert(coroutine.running()==inner);collectgarbage()
      end,'=nested-reader',1)
      assert(s==0 and result()==53)
    end
  end,'=outer-reader',2)
  assert(s==0 and result()==47 and visits==1)
  print('nested-reader',s,b,a,old,new,n,result());count=count+1
end
for _,kind in ipairs{'fresh','completed','yielded','failed'}do
  local co=make(kind)
  local s,b,a,old,new,n,result=boundary_thread_load(co,'return 17',function()coroutine.yield(29)end,'=reader-yield',100)
  assert(s==2 and result=='attempt to yield across metamethod/C-call boundary')
  assert(new==(kind=='fresh'and 0 or 1)and n==(kind=='fresh'and 1 or 2))
  assert(coroutine.status(co)=='suspended')
  print(kind..'-reader-yield',s,b,a,old,new,n,result);count=count+1
end
print('thread-loads-complete',count)
