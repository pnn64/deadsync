local count=0
local function pack(...)return{n=select('#',...),...}end
local function make(kind)
 local co=coroutine.create(function()
  if kind=='failed'then error('old')end
  if kind=='yielded'then coroutine.yield(17)end
  if kind=='partial'then return boundary_execution_yield_tail(11,13,17)end
 end)
 if kind~='fresh'then local ok=coroutine.resume(co);assert(ok==(kind~='failed'))end
 return co
end
local kinds={'fresh','completed','yielded','failed','partial'}
local metas={'__index','__index','__newindex','__newindex','__eq','__lt'}
for _,kind in ipairs(kinds)do for mode=0,5 do for style=0,7 do
 collectgarbage();collectgarbage('stop')
 local co=make(kind);local visits=0;local saved;local destination={field=17}
 local left,right,key,value,expected,called
 key='field';value=53
 local function callback(a,b,c)
  assert(coroutine.running()==co);visits=visits+1
  local held={value=71};saved=function()return held.value end
  if style==7 then collectgarbage();collectgarbage()end
  if mode==2 or mode==3 then assert(b==key and c==value);destination[b]=c;return end
  if mode==4 or mode==5 then return true end
  assert(b==key)
  if style==4 then return nil elseif style==5 then return false elseif style==6 then return-0 else return 31 end
 end
 if mode>3 and style==0 then left=17;right=29;expected=mode==5
 elseif style==0 then left=destination;expected=17
 elseif style==3 and mode<4 then
  left=setmetatable({},{[metas[mode+1]]=setmetatable({},{[metas[mode+1]]=destination})});expected=17
 else
  local meta={[metas[mode+1]]=callback}
  if style==2 then left=newproxy(true);right=newproxy(left);getmetatable(left)[metas[mode+1]]=callback
  else left=setmetatable({},meta);right=setmetatable({},meta)end
  if mode>3 then expected=true elseif style==4 then expected=nil elseif style==5 then expected=false elseif style==6 then expected=-0 else expected=31 end
  called=true
 end
 if mode<4 then right=key end
 local b,a,old,new,result=boundary_table_thread(co,mode,left,right,value)
 assert(old==new)
 local added=mode==2 or mode==3
 assert(a==b+(added and 1 or 2)+(called and kind=='partial'and 2 or 0))
 if added then assert(result==nil and destination.field==53)else assert(result==expected)end
 assert(visits==(called and 1 or 0))
 if saved then assert(saved()==71 and debug.setupvalue(saved,1,{value=73})=='held'and saved()==73)end
 print('table-api',kind,mode,style,b,a,old,new,type(result),result==expected,visits,saved and saved()or 0);count=count+1
end end end
local function token(typ)
 if typ==1 then return{}elseif typ==2 then return'table\0failure'elseif typ==3 then return-0
 elseif typ==4 then return false elseif typ==5 then return nil elseif typ==6 then return function()return 17 end
 elseif typ==7 then return newproxy(false)else return coroutine.create(function()end)end
end
for _,kind in ipairs(kinds)do for mode=0,5 do for typ=1,8 do
 collectgarbage();collectgarbage('stop')
 local co=make(kind);local value=token(typ);local visits=0;local saved
 local function callback()
  assert(coroutine.running()==co);visits=visits+1
  local held={value=71};saved=function()return held.value end
  collectgarbage();error(value,0)
 end
 local meta={[metas[mode+1]]=callback};local left=setmetatable({},meta);local right=mode>3 and setmetatable({},meta)or'field'
 local v=pack(boundary_table_thread_panic(co,value,mode,left,right,53))
 assert(v.n==10 and v[1]==1 and v[2]==2 and v[3]==1 and v[4]==({5,4,3,1,0,6,7,8})[typ])
 assert(v[5]and v[6]and v[7]==0 and v[8]==0 and v[9]==1 and v[10]==2)
 assert(visits==1 and coroutine.status(co)=='dead'and saved()==71)
 assert(debug.setupvalue(saved,1,{value=73})=='held'and saved()==73)
 print('table-panic',kind,mode,typ,unpack(v));count=count+1
end end end
for _,kind in ipairs(kinds)do for mode=0,5 do for _,native in ipairs{false,true}do
 collectgarbage();collectgarbage('stop')
 local co=make(kind);local calls=0
 local function callback()
  local function recurse()
   calls=calls+1
   if native then return boundary_table_invoke(recurse)end
   local ok,e=pcall(recurse);if not ok then error(e,0)end
  end
  local ok,e=pcall(recurse);assert(not ok and e:find('C stack overflow',1,true))
  return mode>3 and true or 31
 end
 local meta={[metas[mode+1]]=callback};local left=setmetatable({},meta);local right=mode>3 and setmetatable({},meta)or'field'
 local b,a,old,new,result=boundary_table_thread(co,mode,left,right,53)
 assert(old==new and calls>0 and(calls<250))
 assert(a==b+((mode==2 or mode==3)and 1 or 2)+(kind=='partial'and 2 or 0))
 print('table-call-count',kind,mode,native,calls,b,a,old,new,type(result));count=count+1
end end end
for _,main in ipairs{false,true}do for mode=0,5 do for _,native in ipairs{false,true}do for _,nested in ipairs{false,true}do
 collectgarbage();collectgarbage('stop')
 local parent,child,saved,observed;local visits=0;local destination={}
 local function body()
  local a,b,c,d=11,13,17,19;local held={value=71};saved=function()return held.value end
  child=coroutine.create(function()
   local function callback(_,key,value)
    assert(main and coroutine.running()==nil or not main and coroutine.running()==parent)
    visits=visits+1;collectgarbage();destination[key]=value
    return mode>3 and true or 31
   end
   local meta={[metas[mode+1]]=callback};local left=setmetatable({},meta);local right=mode>3 and setmetatable({},meta)or'field'
   local function execute()return pack(boundary_table_thread(parent,mode,left,right,53))end
   if nested then local ok;ok,observed=pcall(execute);assert(ok)else observed=execute()end
  end)
  if native then assert(boundary_waiting_c_resume(child)==0)else assert(coroutine.resume(child))end
  assert(a==11 and b==13 and c==17 and d==19 and saved()==71 and coroutine.status(child)=='dead')
 end
 if main then parent=boundary_execution_called();body()
 else parent=coroutine.create(body);assert(coroutine.resume(parent))end
 assert(observed.n==5 and observed[3]==0 and observed[4]==0 and visits==1)
 if mode<2 then assert(observed[5]==31)elseif mode>3 then assert(observed[5]==true)else assert(destination.field==53 and observed[5]==nil)end
 collectgarbage();assert(debug.setupvalue(saved,1,{value=73})=='held'and saved()==73)
 print('table-waiting-success',main,mode,native,nested,observed[1],observed[2],observed[3],observed[4],type(observed[5]),visits,saved());count=count+1
end end end end
local specs={{kind='main',catch='pcall'},{kind='main',catch='xpcall'},
 {kind='co',catch='pcall'},{kind='co',catch='xpcall'},{kind='co',catch='resume'}}
for _,kind in ipairs(kinds)do
 for _,handler in ipairs{false,true}do specs[#specs+1]={kind='api',state=kind,handler=handler}end
end
for _,spec in ipairs(specs)do for mode=0,5 do for _,native in ipairs{false,true}do for _,nested in ipairs{false,true}do for typ=1,8 do
 collectgarbage();collectgarbage('stop')
 local parent,child,saved,metasaved;local value=token(typ);local changed={};local after=false;local visits=0
 local function onerror(e)
  assert(e==value and(typ~=3 or 1/e==-math.huge))
  assert(spec.kind=='main'and coroutine.running()==nil or spec.kind~='main'and coroutine.running()==parent)
  assert(coroutine.status(child)=='normal');visits=visits+1;collectgarbage();assert(saved()==71 and metasaved()==79)
  return changed
 end
 local function body()
  child=coroutine.create(function()
   local held={value=71};saved=function()return held.value end
   local function raise()
    local function fail()
     assert(spec.kind=='main'and coroutine.running()==nil or spec.kind~='main'and coroutine.running()==parent)
     local held={value=79};metasaved=function()return held.value end
     collectgarbage();error(value,0)
    end
    local meta={[metas[mode+1]]=fail};local left=setmetatable({},meta);local right=mode>3 and setmetatable({},meta)or'field'
    boundary_table_thread(parent,mode,left,right,53)
   end
   if nested then local ok=pcall(raise);after=true;return ok else raise();after=true end
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
 assert(visits==(handled and 1 or 0)and saved()==71 and metasaved()==79 and coroutine.status(child)=='normal')
 if typ==3 and not handled then assert(1/e==-math.huge)end
 local status,top,frames,framekinds=boundary_waiting_snapshot(child)
 assert(status==0 and top==5 and frames==(nested and 4 or 3)and framekinds==(nested and'CLCL'or'CLL'))
 local weak=setmetatable({child},{__mode='v'});child=nil;parent=nil;collectgarbage();collectgarbage()
 assert(weak[1]==nil and saved()==71 and metasaved()==79)
 assert(debug.setupvalue(saved,1,{value=73})=='held'and saved()==73)
 assert(debug.setupvalue(metasaved,1,{value=83})=='held'and metasaved()==83)
 print('table-waiting',spec.kind,spec.catch or spec.state,spec.handler or false,mode,native,nested,typ,visits,status,top,frames,framekinds,saved(),metasaved());count=count+1
end end end end end
collectgarbage('restart');collectgarbage()
assert(count==3468)
print('thread-tables-complete',count)
