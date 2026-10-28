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
local function token(typ)
 if typ==1 then return{}elseif typ==2 then return'call\0failure'elseif typ==3 then return-0
 elseif typ==4 then return false elseif typ==5 then return nil elseif typ==6 then return function()return 17 end
 elseif typ==7 then return newproxy(false)else return coroutine.create(function()end)end
end
for _,kind in ipairs{'fresh','completed','yielded','failed','partial'}do
 for mode=0,3 do for _,wanted in ipairs{-1,0,1,3}do for _,nargs in ipairs{0,3}do
  collectgarbage();collectgarbage('stop')
  local co=make(kind);local value={};local negative=token(3);local visits=0;local finalized=0
  local function body(...)
   assert(coroutine.running()==co and select('#',...)==nargs)
   if nargs==3 then local a,b,c=...;assert(a==value and b==nil and c==false)end
   visits=visits+1
   if mode==1 then local ok,e=pcall(error,value,0);assert(not ok and e==value)
   elseif mode==2 then
    do local p=newproxy(true);getmetatable(p).__gc=function()assert(coroutine.running()==co);finalized=finalized+1 end end
    collectgarbage();assert(finalized==1)
   elseif mode==3 then
    local inner=make('completed')
    local b,a,old,new,values=boundary_unprotected_call(inner,function()assert(coroutine.running()==inner);collectgarbage();return value end,1)
    assert(a==b+1 and old==new and values[a]==value)
   end
   return value,nil,negative,false,co
  end
  local b,a,old,new,values
  if nargs==0 then b,a,old,new,values=boundary_unprotected_call(co,body,wanted)
  else b,a,old,new,values=boundary_unprotected_call(co,body,wanted,value,nil,false)end
  local n=wanted==-1 and 5 or wanted;local prefix=b+(kind=='partial'and 2 or 0)
  assert(a==prefix+n and old==new and visits==1 and finalized==(mode==2 and 1 or 0))
  if n>=1 then assert(values[prefix+1]==value)end
  if n>=3 then assert(values[prefix+2]==nil and values[prefix+3]==0 and 1/values[prefix+3]==-math.huge)end
  if n>=5 then assert(values[prefix+4]==false and values[prefix+5]==co)end
  print('thread-call',kind,mode,wanted,nargs,b,a,old,new,visits,finalized);count=count+1
 end end end
end
for _,kind in ipairs{'fresh','completed','yielded','failed','partial'}do
 for typ=1,8 do for _,native in ipairs{false,true}do for callback=1,2 do
  collectgarbage();collectgarbage('stop')
  local co=make(kind);local value=token(typ);local visits=0;local saved
  local function body()
   assert(coroutine.running()==co);visits=visits+1
   local held={value=71};saved=function()return held.value end
   if callback==2 then boundary_panic_swap(co,2)end
   if native then boundary_unprotected_fail(value)else error(value,0)end
  end
  local which,status,top,t,same,identity,caller,frame,after,new=boundary_unprotected_panic(co,value,body)
  local want_type=({5,4,3,1,0,6,7,8})[typ]
  assert(which==callback and status==2 and top==1 and t==want_type and same and identity)
  assert(caller==0 and frame==0 and after==(callback==2 and 2 or 1)and new==2 and visits==1)
  assert(coroutine.status(co)=='dead'and saved()==71)
  collectgarbage();assert(debug.setupvalue(saved,1,{value=73})=='held'and saved()==73)
  print('thread-call-panic',kind,typ,native,callback,which,status,top,t,same,identity,caller,frame,after,new,saved());count=count+1
 end end end
end
local specs={{kind='main',catch='pcall'},{kind='main',catch='xpcall'},
 {kind='co',catch='pcall'},{kind='co',catch='xpcall'},{kind='co',catch='resume'}}
for _,kind in ipairs{'fresh','completed','yielded','failed','partial'}do
 for _,handler in ipairs{false,true}do specs[#specs+1]={kind='api',state=kind,handler=handler}end
end
for _,spec in ipairs(specs)do for typ=1,8 do for _,native in ipairs{false,true}do for _,nested in ipairs{false,true}do
 collectgarbage();collectgarbage('stop')
 local parent,child,saved;local value=token(typ);local changed={};local after=false;local visits=0
 local function onerror(e)
  assert(e==value and(typ~=3 or 1/e==-math.huge));visits=visits+1
  assert(coroutine.status(child)=='normal');collectgarbage();assert(saved()==71)
  return changed
 end
 local function body()
  child=coroutine.create(function()
   local function call()
    boundary_unprotected_call(parent,function()
     assert(spec.kind=='main'and coroutine.running()==nil or spec.kind~='main'and coroutine.running()==parent)
     local held={value=71};saved=function()return held.value end
     if native then boundary_unprotected_fail(value)else error(value,0)end
    end,-1)
   end
   if nested then pcall(call)else call()end
   after=true
  end)
  return coroutine.resume(child)
 end
 local ok,e
 if spec.kind=='main'then
  parent=boundary_execution_called()
  if spec.catch=='xpcall'then ok,e=xpcall(body,onerror)else ok,e=pcall(body)end
 elseif spec.kind=='co'then
  parent=coroutine.create(function()
   if spec.catch=='xpcall'then return xpcall(body,onerror)
   elseif spec.catch=='pcall'then return pcall(body)else return body()end
  end)
  if spec.catch=='resume'then ok,e=coroutine.resume(parent)
  else local resumed;resumed,ok,e=coroutine.resume(parent);assert(resumed)end
 else
  parent=make(spec.state)
  local status,values,b,a,old,new=boundary_thread_pcall(parent,body,-1,spec.handler and onerror or nil)
  assert(status==2 and old==new);ok=false;e=values[a]
 end
 local handled=spec.catch=='xpcall'or spec.handler
 assert(ok==false and e==(handled and changed or value)and not after and visits==(handled and 1 or 0))
 assert(saved()==71 and coroutine.status(child)=='normal')
 local status,top,frames,kinds=boundary_waiting_snapshot(child)
 assert(status==0 and top==3)
 local weak=setmetatable({child},{__mode='v'});child=nil;parent=nil;collectgarbage();collectgarbage()
 assert(weak[1]==nil and saved()==71);assert(debug.setupvalue(saved,1,{value=73})=='held'and saved()==73)
 print('waiting-call-error',spec.kind,spec.catch or spec.state,spec.handler or false,typ,native,nested,visits,status,top,frames,kinds,saved());count=count+1
end end end end
for _,kind in ipairs{'fresh','completed','yielded','failed','partial'}do
 for typ=1,8 do for _,native in ipairs{false,true}do for callback=1,2 do
  collectgarbage();collectgarbage('stop')
  local parent=make(kind);local main=boundary_execution_called();local value=token(typ);local escape={}
  local child,saved_parent,saved_child;local after=false
  boundary_panic_swap(parent,callback)
  local ok,e=pcall(function()
   boundary_unprotected_call(parent,function()
    local held={value=71};saved_parent=function()return held.value end
    child=coroutine.create(function()
     local held={value=79};saved_child=function()return held.value end
     local which,status,top,t,same,identity,caller,frame,after,new=boundary_panic_raise(parent,value)
     assert(which==callback and status==2 and top==1 and t==({5,4,3,1,0,6,7,8})[typ]and same and identity)
     assert(caller==0 and frame==0 and after==(callback==2 and 2 or 1)and new==2)
     assert(coroutine.status(parent)=='dead'and saved_parent()==71)
     boundary_waiting_raise(main,escape)
    end)
    if native then boundary_waiting_c_resume(child)else coroutine.resume(child)end
    after=true
   end,-1)
   after=true
  end)
  boundary_panic_swap(nil,4)
  assert(not ok and e==escape and not after)
  assert(saved_parent()==71 and saved_child()==79 and coroutine.status(parent)=='dead'and coroutine.status(child)=='normal')
  local status,top,frames,kinds=boundary_waiting_snapshot(parent)
  local a,b,c,d=boundary_waiting_snapshot(child)
  assert(status==2 and top==(callback==2 and 2 or 1)and frames==0 and kinds=='')
  assert(a==0 and b==2 and c==2 and d=='CL')
  local weak=setmetatable({parent,child},{__mode='v'});parent=nil;child=nil;main=nil;collectgarbage();collectgarbage()
  assert(not weak[1]and not weak[2]and saved_parent()==71 and saved_child()==79)
  print('waiting-call-panic',kind,typ,native,callback,status,top,frames,a,b,c,d,saved_parent(),saved_child());count=count+1
 end end end
end
assert(count==960)
print('thread-unprotected-complete',count)
