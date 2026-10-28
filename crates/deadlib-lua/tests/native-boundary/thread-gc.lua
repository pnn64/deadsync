local count=0
local function make(kind)
  local co=coroutine.create(function()
    if kind=='failed'then error('old')end
    if kind=='yielded'then coroutine.yield(17)end
    if kind=='partial'then return boundary_execution_yield_tail(11,13,17)end
  end)
  if kind~='fresh'then local ok=coroutine.resume(co);assert(ok==(kind~='failed'))end
  return co
end
for _,kind in ipairs{'fresh','completed','yielded','failed','partial'}do
  for _,mode in ipairs{0,1,2,3,4,6,7,99}do
    collectgarbage();collectgarbage('stop')
    local co=make(kind)
    collectgarbage('setpause',137);collectgarbage('setstepmul',139)
    local r,b,a,old,new=boundary_gc_thread(co,mode,151)
    local result=r
    if mode==3 or mode==4 then
      assert(type(r)=='number'and r>=0 and r==math.floor(r)and(mode==3 or r<1024));result='range-valid'
    else assert(r==(mode==6 and 137 or mode==7 and 139 or mode==99 and-1 or 0))end
    assert(a==b and old==new)
    print('gc-control',kind,mode,result,b,a,old,new);count=count+1
    collectgarbage('setpause',200);collectgarbage('setstepmul',200);collectgarbage()
  end
  for _,mode in ipairs{2,5}do
    collectgarbage();collectgarbage('stop')
    local co=make(kind);local visits=0;local context=false;local parent='none'
    do local p=newproxy(true);getmetatable(p).__gc=function()
      visits=visits+1;context=coroutine.running()==co
      local info=debug.getinfo(2,'f')
      if info then parent=info.func==coroutine.yield and'yield'or info.func==error and'error'
        or info.func==boundary_execution_yield_tail and'partial'or'other'end
    end end
    local r,b,a,old,new=boundary_gc_thread(co,mode,2147483647)
    assert(r==(mode==5 and 1 or 0)and old==new and a==b+(kind=='partial'and 2 or 0))
    assert(visits==1 and context)
    assert(parent==((kind=='fresh'or kind=='completed')and'none'or kind=='yielded'and'yield'or kind=='failed'and'error'or'partial'))
    print('gc-finalizer',kind,mode,r,b,a,old,new,visits,context,parent);count=count+1
    collectgarbage()
  end
end
for _,mode in ipairs{2,5}do
  collectgarbage();collectgarbage('stop')
  local parent=boundary_execution_called();local visits=0
  local co=coroutine.create(function()
    do local p=newproxy(true);getmetatable(p).__gc=function()
      assert(coroutine.running()==nil);visits=visits+1
    end end
    local r,b,a,old,new=boundary_gc_thread(parent,mode,2147483647)
    assert(r==(mode==5 and 1 or 0)and a==b and old==new and visits==1)
    print('gc-waiting',mode,r,b,a,old,new,visits);count=count+1
  end)
  local ok,e=coroutine.resume(co);assert(ok,tostring(e));collectgarbage()
end
assert(count==52)
print('thread-gc-complete',count)
