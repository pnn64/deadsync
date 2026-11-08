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
 if typ==1 then return{}elseif typ==2 then return'waiting\0error'elseif typ==3 then return-0
 elseif typ==4 then return false elseif typ==5 then return nil elseif typ==6 then return function()return 17 end
 elseif typ==7 then return newproxy(false)else return coroutine.create(function()end)end
end
local specs={{kind='main',catch='pcall'},{kind='main',catch='xpcall'},
 {kind='co',catch='pcall'},{kind='co',catch='xpcall'},{kind='co',catch='resume'}}
for _,kind in ipairs{'fresh','completed','yielded','failed','partial'}do
 for _,handler in ipairs{false,true}do specs[#specs+1]={kind='api',state=kind,handler=handler}end
end
local function run(spec,native,nested,typ,operation)
 collectgarbage();collectgarbage('stop')
 local parent,child,saved;local value=token(typ);local changed={};local after=false;local visits=0
 local function onerror(e)
  assert(e==value and(typ~=3 or 1/e==-math.huge))
  assert(spec.kind=='main'and coroutine.running()==nil or spec.kind~='main'and coroutine.running()==parent)
  assert(coroutine.status(child)=='normal');visits=visits+1;collectgarbage();assert(saved()==71)
  return changed
 end
 local function body()
  child=coroutine.create(function()
   local held={value=71};saved=function()return held.value end
   local function raise()
    if operation==0 then boundary_waiting_raise(parent,value)
    else
     do local p=newproxy(true);getmetatable(p).__gc=function()error(value,0)end end
     boundary_gc_thread(parent,operation,2147483647)
    end
   end
   if nested then local ok=pcall(raise);after=true;return ok
   else raise();after=true end
  end)
  if native then return boundary_waiting_c_resume(child)else return coroutine.resume(child)end
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
 assert(ok==false and e==(handled and changed or value)and after==false)
 assert(visits==(handled and 1 or 0)and saved()==71 and coroutine.status(child)=='normal')
 if typ==3 and not handled then assert(1/e==-math.huge)end
 local status,top,frames,kinds=boundary_waiting_snapshot(child)
 local want=operation==0 and 2 or 3
 assert(status==0 and top==want and frames==(nested and 4 or 3)and kinds==(nested and'CLCL'or'CLL'))
 local weak=setmetatable({child},{__mode='v'});child=nil;parent=nil;collectgarbage();collectgarbage()
 assert(weak[1]==nil and saved()==71);assert(debug.setupvalue(saved,1,{value=73})=='held'and saved()==73)
 print('waiting-error',spec.kind,spec.catch or spec.state,spec.handler or false,native,nested,typ,operation,ok,not after,visits,status,top,frames,kinds,saved());count=count+1
end
for _,spec in ipairs(specs)do for _,native in ipairs{false,true}do for _,nested in ipairs{false,true}do for typ=1,8 do
 run(spec,native,nested,typ,0)
end end end end
for _,spec in ipairs{specs[1],specs[5],specs[8]}do for _,mode in ipairs{2,5}do for _,native in ipairs{false,true}do for _,nested in ipairs{false,true}do for typ=1,8 do
 run(spec,native,nested,typ,mode)
end end end end end
for _,escalate in ipairs{false,true}do for _,native in ipairs{false,true}do for _,nested in ipairs{false,true}do for typ=1,8 do
 collectgarbage();collectgarbage('stop')
 local main=boundary_execution_called();local value=token(typ);local first={};local middle,inner,sm,si
 local after=false;local handled=0
 middle=coroutine.create(function()
  local mh={value=79};sm=function()return mh.value end
  local function body()
   inner=coroutine.create(function()
    local ih={value=71};si=function()return ih.value end
    local destination=escalate and middle or main
    if nested then pcall(function()boundary_waiting_raise(destination,escalate and first or value)end)
    else boundary_waiting_raise(destination,escalate and first or value)end
    after=true
   end)
   if native then boundary_waiting_c_resume(inner)else coroutine.resume(inner)end
   after=true
  end
  if escalate then xpcall(body,function(e)
   assert(e==first and coroutine.running()==middle and coroutine.status(inner)=='normal')
   collectgarbage();assert(si()==71);handled=handled+1
   boundary_waiting_raise(main,value);after=true
  end)else pcall(body)end
  after=true
 end)
 local ok,e=pcall(function()if native then return boundary_waiting_c_resume(middle)else return coroutine.resume(middle)end end)
 assert(ok==false and e==value and not after and handled==(escalate and 1 or 0))
 if typ==3 then assert(1/e==-math.huge)end
 assert(coroutine.status(inner)=='normal'and coroutine.status(middle)=='normal')
 local a,b,c,d=boundary_waiting_snapshot(inner);local x,y,z,w=boundary_waiting_snapshot(middle)
 assert(a==0 and b==2 and c==(nested and 4 or 2)and d==(nested and'CLCL'or'CL'))
 assert(x==0 and y==(escalate and 2 or 1)and z==(escalate and 6 or 4)and w==(escalate and'CLCLCL'or'CLCL'))
 local weak=setmetatable({middle,inner},{__mode='v'});middle=nil;inner=nil;collectgarbage();collectgarbage()
 assert(not weak[1]and not weak[2]and sm()==79 and si()==71)
 print('waiting-chain',escalate,native,nested,typ,ok,not after,handled,a,b,c,d,x,y,z,w,sm(),si());count=count+1
end end end end
assert(count==736)
print('waiting-errors-complete',count)
