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
local metas={'__index','__index','__newindex','__newindex','__eq','__lt'}
if not table_yields_portable then
for _,kind in ipairs{'fresh','completed','yielded','failed','partial'}do for mode=0,5 do for n=0,4 do
 collectgarbage();collectgarbage('stop')
 local co=make(kind);local saved;local visits=0
 local function callback()
  assert(coroutine.running()==co);visits=visits+1
  local held={value=71};saved=function()return held end
  coroutine.yield(unpack({'one','two','three','four'},1,n));return 53
 end
 local meta={[metas[mode+1]]=callback};local left=setmetatable({},meta);local right=mode>3 and setmetatable({},meta)or'field'
 local v=pack(boundary_table_thread_panic(co,'attempt to yield across metamethod/C-call boundary',mode,left,right,73))
 assert(v.n==10 and visits==1);local first=type(saved())
 collectgarbage();local during=type(saved())
 print('table-yield',kind,mode,n,first,during,unpack(v));count=count+1
 local weak=setmetatable({co},{__mode='v'});co=nil;left=nil;right=nil;meta=nil
 collectgarbage();collectgarbage();assert(weak[1]==nil)
 local replacement={value=73};assert(debug.setupvalue(saved,1,replacement)=='held'and saved()==replacement)
end end end
end
local kinds=table_yields_portable and{'completed','yielded','failed','partial'}or{'fresh','completed','yielded','failed','partial'}
for _,kind in ipairs(kinds)do for mode=0,5 do for n=0,4 do for style=1,5 do
 collectgarbage();collectgarbage('stop')
 local co=make(kind);local saved;local visits=0;local tokens
 local function callback()
  assert(coroutine.running()==co);visits=visits+1
  local held={value=71};saved=function()return held end
  coroutine.yield(unpack(tokens,1,n));return 53
 end
 local meta={[metas[mode+1]]=callback};local left=setmetatable({},meta);local right=mode>3 and setmetatable({},meta)or'field'
 if style==1 then tokens={11,13,17,19}elseif style==2 then tokens={false,false,false,false}
 elseif style==3 then tokens={}elseif style==4 then tokens={left,right,callback,coroutine.yield}
 else tokens={'a\0b',-0,newproxy(false),coroutine.create(function()end)}end
 local v=pack(boundary_table_yield(co,'attempt to yield across metamethod/C-call boundary',mode,left,right,73))
 local expected=false;if kind~='fresh'and mode>3 then expected=n==0 or not not tokens[n]end
 assert(v.n==14 and visits==1 and type(saved())=='table'and v[13]==expected)
 if kind=='fresh'then
  assert(v[1]==1 and v[2]==2 and v[3]==1 and v[4]==4 and v[5]and v[6]and v[7]==0 and v[8]==0 and v[9]==1 and v[10]==2)
  assert(v[14][1]=='attempt to yield across metamethod/C-call boundary')
 else
  local removed=mode==1 and 0 or mode==2 and 2 or 1
  assert(v[1]==0 and v[2]==0 and v[3]==0 and v[4]==0 and not v[5]and not v[6]and v[7]==0 and v[8]==0 and v[9]==n-removed and v[10]==1)
  for i=1,math.max(v[9],0)do assert(rawequal(v[14][i],tokens[i]));if tokens[i]==0 then assert(1/v[14][i]==1/tokens[i])end end
 end
 collectgarbage();local during=type(saved())
 assert(during==(kind~='fresh'and mode==2 and n==0 and'nil'or'table'))
 print('table-yield-values',kind,mode,n,style,v[9],v[10],v[11],v[12],v[13],during);count=count+1
 co=nil;left=nil;right=nil;meta=nil;tokens=nil;v=nil;collectgarbage();collectgarbage()
 local replacement={value=73};assert(debug.setupvalue(saved,1,replacement)=='held'and saved()==replacement)
end end end end
for _,kind in ipairs(kinds)do for mode=0,5 do for style=1,3 do
 collectgarbage();collectgarbage('stop')
 local co=make(kind);local callback=({boundary_table_c_yield_all,boundary_table_c_yield_tail,boundary_table_c_yield_zero})[style]
 local meta={[metas[mode+1]]=callback};local left=setmetatable({},meta);local right=mode>3 and setmetatable({},meta)or'field'
 local tokens=mode>3 and{left,right}or(mode==2 or mode==3)and{left,'field',73}or{left,'field'}
 local n=#tokens;if style==2 then tokens={tokens[n]};n=1 elseif style==3 then tokens={};n=0 end
 local v=pack(boundary_table_yield(co,'attempt to yield across metamethod/C-call boundary',mode,left,right,73))
 local expected=false;if kind~='fresh'and mode>3 then expected=n==0 or not not tokens[n]end
 assert(v.n==14 and v[13]==expected)
 if kind=='fresh'then assert(v[1]==1 and v[2]==2 and v[3]==1 and v[4]==4 and v[5]and v[6]and v[9]==1 and v[10]==2)
 else
  local removed=mode==1 and 0 or mode==2 and 2 or 1
  assert(v[1]==0 and v[9]==n-removed and v[10]==1)
  for i=1,math.max(v[9],0)do assert(rawequal(v[14][i],tokens[i]))end
 end
 print('table-c-yield',kind,mode,style,n,v[9],v[10],v[11],v[12],v[13]);count=count+1
 local weak=setmetatable({co},{__mode='v'});co=nil;left=nil;right=nil;meta=nil;tokens=nil;v=nil
 collectgarbage();collectgarbage();assert(weak[1]==nil)
end end end
for _,mode in ipairs{0,2,3}do for n=1,4 do if mode~=2 or n>=2 then for argc=0,3 do for _,repeated in ipairs{false,true}do
 collectgarbage();collectgarbage('stop')
 local co=make('completed');local saved;local resumed=0;local names={'one','two','three','four'};local args={41,43,47}
 local function callback()
  local held={value=71};saved=function()return held end
  local a,b,c=coroutine.yield(unpack(names,1,n))
  assert(a==(argc>0 and args[1]or nil)and b==(argc>1 and args[2]or nil)and c==(argc>2 and args[3]or nil))
  assert(held.value==73);resumed=resumed+1
  if repeated then local a,b=coroutine.yield('again',79);assert(a==83 and b==89);resumed=resumed+1 end
  return'done'
 end
 local meta={[metas[mode+1]]=callback};local left=setmetatable({},meta)
 local first=pack(boundary_table_yield(co,'unused',mode,left,'field',73))
 assert(first.n==14 and first[1]==0 and first[10]==1 and saved().value==71)
 collectgarbage();local replacement={value=73};assert(debug.setupvalue(saved,1,replacement)=='held')
 local result=pack(coroutine.resume(co,unpack(args,1,argc)))
 if repeated then
  assert(result.n==3 and result[1]and result[2]=='again'and result[3]==79)
  collectgarbage();result=pack(coroutine.resume(co,83,89))
 end
 assert(result[1]and resumed==(repeated and 2 or 1)and coroutine.status(co)=='dead'and saved()==replacement)
 assert(rawequal(result[2],left))
 if mode==0 then assert(result.n==4 and result[3]==names[n]and result[4]=='done')
 elseif mode==2 then assert(result.n==4 and result[3]=='field'and result[4]==73)
 else assert(result.n==3 and result[3]==73)end
 print('table-yield-resume',mode,n,argc,repeated,resumed,result.n,type(result[2]),result[3],result[4]);count=count+1
end end end end end
collectgarbage('restart');collectgarbage()
assert(count==(table_yields_portable and 760 or 1078))
print('table-yields-complete',count)
