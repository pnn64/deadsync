local count=0
-- The reference debug hook registry uses pointer keys. Keep hooked targets
-- alive so a later empty child cannot reuse a collected target's hook entry.
local retained={}
local function make(kind)
  local co=coroutine.create(function()
    if kind=='failed'then error('old')end
    if kind=='yielded'then coroutine.yield(17)end
    if kind=='partial'then return boundary_execution_yield_tail(11,13,17)end
    return 23
  end)
  if kind~='fresh'then local ok=coroutine.resume(co);assert(ok==(kind~='failed'))end
  return co
end
for _,kind in ipairs{'fresh','completed','yielded','failed','partial'}do
  do
    local co=make(kind);local env=setmetatable({marker=31},{__index=_G});debug.setfenv(co,env)
    retained[#retained+1]=co
    debug.sethook(co,function()end,'cr',21)
    local b,a,old,new,same,inherited,child=boundary_execution_construct(co)
    retained[#retained+1]=child
    assert(a==b+1 and old==new and same and inherited and debug.getfenv(child)==env and coroutine.status(child)=='dead')
    assert(coroutine.resume(child)==false)
    local hook,mask,n=debug.gethook(child);assert(hook==nil and mask=='cr'and n==21)
    print(kind,'construct',b,a,old,new,same,inherited,coroutine.status(child),type(hook),mask,n);count=count+1
  end
  do
    local co=make(kind);local b,a,old,new,text=boundary_execution_format(co)
    assert(a==b+1 and old==new and text=='value:17:3.5:A:%')
    print(kind,'format',b,a,old,new,text);count=count+1
  end
  for _,values in ipairs{{},{'a'},{17},{'a',17},{'a',17,'b',3.5}}do
    local co=make(kind);local b,a,old,new,text=boundary_execution_concat(co,unpack(values))
    local expected=#values==0 and''or #values==1 and values[1]or #values==2 and'a17'or'a17b3.5'
    assert(a==b+1 and old==new and text==expected)
    print(kind,'concat',#values,b,a,old,new,type(text),text);count=count+1
  end
  for variant=1,3 do
    local co=make(kind);local visits=0
    local mt={__concat=function(a,b)
      assert(coroutine.running()==co);visits=visits+1;collectgarbage();return'joined'
    end}
    local x=setmetatable({},mt);local y=setmetatable({},mt);local z=setmetatable({},mt)
    local values=variant==1 and{x,y}or variant==2 and{'a',x,'b',y}or{x,y,z}
    local b,a,old,new,text=boundary_execution_concat(co,unpack(values))
    local expected=kind=='partial'and(variant==2 and'a'or variant==3 and x or'joined')
      or(variant==2 and'ajoined'or'joined')
    assert(old==new and a==b+(kind=='partial'and 3 or 1))
    assert(visits==((kind=='partial'or variant==1)and 1 or 2)and text==expected)
    print(kind,'metamethod',variant,b,a,old,new,visits,type(text),type(text)=='table'and text==x and'operand-x'or text);count=count+1
  end
end
do
  local parent=boundary_execution_called()
  local co=coroutine.create(function()
    local b,a,old,new,same,env,child=boundary_execution_construct(parent)
    assert(a==b+1 and old==new and same and env)
    print('waiting-construct',b,a,old,new,same,env);count=count+1
    local b,a,old,new,text=boundary_execution_format(parent);assert(a==b+1 and old==new)
    print('waiting-format',b,a,old,new,text);count=count+1
    local x=setmetatable({},{__concat=function()assert(coroutine.running()==nil);collectgarbage();return'parent'end})
    local b,a,old,new,text=boundary_execution_concat(parent,x,'x');assert(a==b+1 and old==new and text=='parent')
    print('waiting-concat',b,a,old,new,text);count=count+1
  end)
  local ok,e=coroutine.resume(co);assert(ok,tostring(e))
end
do
  local outer=make('completed');local inner=make('completed');local visits=0
  local x=setmetatable({},{__concat=function()
    visits=visits+1;assert(coroutine.running()==outer)
    local y=setmetatable({},{__concat=function()assert(coroutine.running()==inner);return'inside'end})
    local b,a,old,new,text=boundary_execution_concat(inner,y,'x');assert(text=='inside')
    return'outside'
  end})
  local b,a,old,new,text=boundary_execution_concat(outer,x,'x');assert(text=='outside'and visits==1)
  print('nested-concat',b,a,old,new,text);count=count+1
end
print('thread-execution-complete',count)
