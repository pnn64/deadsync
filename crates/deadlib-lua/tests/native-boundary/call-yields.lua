local count=0
local function pack(...)return{n=select('#',...),...}end
local function negative()return-0 end
local function completed()
 local co=coroutine.create(function()end);assert(coroutine.resume(co));return co
end
local names={'one','two','three'}
local function token(typ)
 if typ==1 then return{}elseif typ==2 then return'call\0resume'elseif typ==3 then return negative()
 elseif typ==4 then return false elseif typ==5 then return nil elseif typ==6 then return function()return 17 end
 elseif typ==7 then return newproxy(false)else return coroutine.create(function()end)end
end
for n=0,3 do for _,wanted in ipairs{-1,0,1,3,6,8}do for _,protected in ipairs{false,true}do
 for _,native in ipairs{false,true}do for _,repeated in ipairs{false,true}do for interleave=0,3 do
  collectgarbage();collectgarbage('stop')
  local co=completed();local saved;local visits=0;local handler_calls=0;local marker={}
  local handler=function(e)handler_calls=handler_calls+1;return e end
  local function body(...)
   assert(coroutine.running()==co and select('#',...)==3)
   local a,b,c=...;assert(a==marker and b==nil and c==false)
   local held={value=71};saved=function()return held.value end
   local a,b=coroutine.yield(unpack(names,1,n));assert(a==41 and b==43 and held.value==73)
   visits=visits+1
   if repeated then
    local c,d=coroutine.yield('again',79);assert(c==83 and d==89);visits=visits+1
   end
   return held.value,a,b,false,negative(),visits
  end
  local status,b,a,old,new,values=boundary_yield_call(co,body,wanted,protected,protected and handler or nil,marker,nil,false)
  assert(status==0 and b==0 and a==n and old==0 and new==1 and handler_calls==0 and visits==0)
  for i=1,n do assert(values[i]==names[i])end
  collectgarbage();assert(saved()==71);assert(debug.setupvalue(saved,1,{value=73})=='held')
  if interleave==1 then
   local s,b,a,old,new,v=boundary_yield_call(co,function()assert(coroutine.running()==co);collectgarbage();return marker,nil,17 end,3,false,nil)
   assert(s==0 and a==b+3 and old==1 and new==1 and v[a-2]==marker and v[a-1]==nil and v[a]==17)
  elseif interleave==2 then
   local changed={};local hits=0
   local s,b,a,old,new,v=boundary_yield_call(co,function()assert(coroutine.running()==co);error(marker,0)end,-1,true,function(e)assert(e==marker);hits=hits+1;collectgarbage();return changed end)
   assert(s==2 and a==b+2 and old==1 and new==1 and v[a]==changed and hits==1)
  elseif interleave==3 then
   local other=completed()
   local s,b,a,old,new,v=boundary_yield_call(other,function()
    assert(coroutine.running()==other)
    local s,b,a,old,new,v=boundary_yield_call(co,function()assert(coroutine.running()==co);collectgarbage();return marker end,1,false,nil)
    assert(s==0 and a==b+1 and old==1 and new==1 and v[a]==marker)
    return marker
   end,1,false,nil)
   assert(s==0 and a==b+1 and old==0 and new==0 and v[a]==marker)
  end
  assert(saved()==73 and handler_calls==0 and coroutine.status(co)=='suspended')
  local resume=native and boundary_yield_resume or coroutine.resume
  local result=pack(resume(co,41,43))
  if repeated then
   assert(result.n==3 and result[1]and result[2]=='again'and result[3]==79)
   collectgarbage();result=pack(resume(co,83,89))
  end
  local total=wanted==-1 and 6 or wanted;local prefix=protected and 1 or 0
  assert(result.n==1+prefix+total and result[1]and coroutine.status(co)=='dead'and visits==(repeated and 2 or 1)and handler_calls==0)
  if protected then assert(result[2]==handler)end
  local expected={73,41,43,false,negative(),visits}
  for i=1,total do assert(result[1+prefix+i]==expected[i])end
  if total>=5 then assert(1/result[1+prefix+5]==-math.huge)end
  local weak=setmetatable({co},{__mode='v'});co=nil;body=nil;collectgarbage();collectgarbage();assert(not weak[1]and saved()==73)
  print('call-yield-resume',n,wanted,protected,native,repeated,interleave,status,a,new,result.n,visits,saved());count=count+1
 end end end
end end end
for n=0,3 do for _,second_n in ipairs{0,3}do for _,first_protected in ipairs{false,true}do
 for _,second_protected in ipairs{false,true}do for _,native in ipairs{false,true}do for typ=1,8 do
  collectgarbage();collectgarbage('stop')
  local co=completed();local value=token(typ);local first,second;local handler_calls=0;local after=false
  local handler=function(e)handler_calls=handler_calls+1;return e end
  local s,b,a,old,new=boundary_yield_call(co,function()
   local held={value=71};first=function()return held.value end
   coroutine.yield(unpack(names,1,n));after=true;return 17
  end,-1,first_protected,first_protected and handler or nil)
  assert(s==0 and b==0 and a==n and old==0 and new==1)
  local s,b,a,old,new=boundary_yield_call(co,function()
   local held={value=79};second=function()return held.value end
   local a,b=coroutine.yield(unpack(names,1,second_n));assert(a==41 and b==43)
   assert(held.value==83);error(value,0)
  end,-1,second_protected,second_protected and handler or nil)
  assert(s==0 and b==n and a==second_n and old==1 and new==1)
  collectgarbage();assert(first()==71 and second()==79)
  assert(debug.setupvalue(second,1,{value=83})=='held')
  local resume=native and boundary_yield_resume or coroutine.resume
  local result=pack(resume(co,41,43))
  assert(result.n==2 and result[1]==false and result[2]==value and handler_calls==0 and not after)
  if typ==3 then assert(1/result[2]==-math.huge)end
  assert(coroutine.status(co)=='dead'and first()==71 and second()==83)
  local status,top,frames,kinds=boundary_waiting_snapshot(co)
  assert(status==2 and top==1 and frames==4 and kinds=='CLCL')
  local weak=setmetatable({co},{__mode='v'});co=nil;collectgarbage();collectgarbage()
  assert(not weak[1]and first()==71 and second()==83)
  print('interleaved-yield-error',n,second_n,first_protected,second_protected,native,typ,handler_calls,status,top,frames,kinds,first(),second());count=count+1
 end end end
end end end
assert(count==1280)
print('call-yields-complete',count)
