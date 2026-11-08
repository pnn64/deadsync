local count=0
local bits=string.byte(string.dump(function()end),9)*8
local function pack(...)return{n=select('#',...),...}end
local function token(typ)
 if typ==1 then return{}elseif typ==2 then return'collector\0failure'elseif typ==3 then return-0
 elseif typ==4 then return false elseif typ==5 then return nil elseif typ==6 then return function()return 17 end
 elseif typ==7 then return newproxy(false)else return coroutine.create(function()end)end
end
local function make(kind)
 if kind=='main'then return nil end
 local co=coroutine.create(function()
  if kind=='failed'then error('old')end
  if kind=='yielded'then coroutine.yield(17)end
  if kind=='partial'then return boundary_execution_yield_tail(11,13,17)end
 end)
 if kind~='fresh'then local ok=coroutine.resume(co);assert(ok==(kind~='failed'))end
 return co
end
local function control(co,which,n)
 if co then
  local result,b,a,old,new=boundary_gc_thread(co,which=='pause'and 6 or 7,n)
  assert(a==b and old==new);return result
 else return collectgarbage(which=='pause'and'setpause'or'setstepmul',n)end
end
local multipliers={0,1,99,100,200,-1,-2,-100,-429496729,429496729,-858993459,858993459,-1073741824,1073741824,-2147483648,2147483647,-2000000000,2000000000}
local function step_case(kind,mul,typ)
 collectgarbage('setpause',200);collectgarbage('setstepmul',200);collectgarbage()
 local co=make(kind);local keep={};for i=1,2048 do keep[i]={i}end
 collectgarbage();collectgarbage('stop')
 local value=token(typ);local visits=0;local armed=true;local saved
 do local p=newproxy(true);getmetatable(p).__gc=function()
  if armed then assert(coroutine.running()==co)end;visits=visits+1
  local held={value=71};saved=function()return held.value end
  if armed and typ>0 then error(value,0)end
 end end
 assert(control(co,'step',mul)==200)
 local raw=(mul*10)%4294967296
 local complete=raw==0 or(raw>100000000 and(bits==64 or raw<2147483648))
 local ok,result,callback,status,top
 if co then
  local v=pack(boundary_gc_thread_panic(co,value,5,0));assert(v.n==10)
  callback,status,top=v[1],v[2],v[9]
  if complete and typ>0 then
   assert(v[1]==1 and v[2]==2 and v[3]==1 and v[4]==({5,4,3,1,0,6,7,8})[typ])
   assert(v[5]and v[6]and v[7]==0 and v[8]==0 and v[9]==1 and v[10]==2)
   assert(coroutine.status(co)=='dead')
  else
   assert(v[1]==0 and v[2]==0 and v[3]==0 and v[4]==0 and not v[5]and not v[6])
   local before=kind=='fresh'and 1 or kind=='failed'and 2 or 0
   local old=kind=='yielded'or kind=='partial';local expected_status=old and 1 or kind=='failed'and 2 or 0
   assert(v[7]==0 and v[8]==0 and v[9]==before+(complete and kind=='partial'and 2 or 0)and v[10]==expected_status)
  end
 else
  ok,result=pcall(collectgarbage,'step',0)
  assert(ok==(not complete or typ==0))
  if ok then assert(result==complete)else assert(result==value and(typ~=3 or 1/result==-math.huge))end
 end
 assert(visits==(complete and 1 or 0)and keep[2048][1]==2048)
 if complete then assert(saved()==71)end
 armed=false;collectgarbage('setstepmul',200);collectgarbage();collectgarbage()
 assert(visits==1 and saved()==71);assert(debug.setupvalue(saved,1,{value=73})=='held'and saved()==73)
 print('gc-step-integer',bits,kind,mul,typ,complete,ok,result==value,callback,status,top,visits,saved());count=count+1
end
for _,kind in ipairs{'main','fresh','completed','yielded','failed','partial'}do
 for _,mul in ipairs(multipliers)do for typ=0,8 do step_case(kind,mul,typ)end end
end
local function pause_case(kind,pause,when)
 collectgarbage('setpause',200);collectgarbage('setstepmul',200);collectgarbage();collectgarbage('stop')
 local co=make(kind);local weak=setmetatable({},{__mode='v'});local keep={}
 for i=1,1000 do local t={};weak[i]=t;keep[i]=t end
 if when=='after'then collectgarbage();assert(control(co,'pause',pause)==200)
 else assert(control(co,'pause',pause)==200);collectgarbage()end
 keep=nil
 if when=='restart'then collectgarbage('stop');collectgarbage('restart')end
 for i=1,10000 do local t={}end
 local automatic=next(weak)==nil
 assert(automatic==(pause>=0 or when~='before'))
 collectgarbage('setpause',200);collectgarbage();assert(next(weak)==nil)
 print('gc-pause-integer',bits,kind,pause,when,automatic);count=count+1
end
for _,kind in ipairs{'main','fresh','completed','yielded','failed','partial'}do
 for _,pause in ipairs{-400,-100,-1,0,200}do for _,when in ipairs{'before','after','restart'}do pause_case(kind,pause,when)end end
end
do
 collectgarbage('setpause',200);collectgarbage('setstepmul',200);collectgarbage();collectgarbage('stop')
 local keep={};for i=1,150000 do keep[i]={i}end
 collectgarbage();collectgarbage('stop')
 local visits=0;do local p=newproxy(true);getmetatable(p).__gc=function()visits=visits+1 end end
 assert(collectgarbage('setstepmul',-2000000000)==200)
 local done=collectgarbage('step',0)
 assert(done and visits==1 and keep[150000][1]==150000)
 print('gc-wide-budget',bits,done,visits,keep[150000][1]);count=count+1
 collectgarbage('setstepmul',200);keep=nil;collectgarbage();collectgarbage()
end
assert(count==1063)
print('gc-integers-complete',count,bits)
