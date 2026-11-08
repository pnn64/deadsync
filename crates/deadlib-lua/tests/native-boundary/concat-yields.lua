local count=0
local function pack(...)return{n=select('#',...),...}end
local message='attempt to yield across metamethod/C-call boundary'
local names={'one','two','three'}
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
  for n=0,3 do
    local co=make(kind);local visited=0
    local x=setmetatable({},{__concat=function()
      coroutine.yield(unpack(names,1,n));visited=visited+1;return'done'
    end})
    local a=pack(boundary_execution_concat_panic(co,message,x,'x'))
    assert(a.n==10 and visited==0)
    if kind=='fresh'then
      assert(a[1]==1 and a[2]==2 and a[3]==1 and a[4]==4 and a[5]and a[6])
      assert(a[7]==0 and a[8]==0 and a[9]==1 and a[10]==2)
    else
      assert(a[1]==0 and a[2]==0 and a[3]==0 and a[4]==0 and not a[5]and not a[6])
      assert(a[7]==0 and a[8]==0 and a[9]==n-2 and a[10]==1)
    end
    print('concat-yield',kind,n,unpack(a,1,a.n));count=count+1
    if kind~='fresh'then
      local b,c,d,e,old,new,top=boundary_concat_offsets(co)
      assert(b==n-2 and c==b+1 and d==b+2 and e==b+3 and old==1 and new==1 and top==0)
      print('offsets',kind,n,b,c,d,e,old,new,top);count=count+1
    end
    collectgarbage()
  end
end
for n=2,3 do
  for _,repeated in ipairs{false,true}do
    local co=make('completed');local resumed=0;local exported
    local x=setmetatable({},{__concat=function()
      local held={value=71};exported=function()return held.value end
      local a,b=coroutine.yield(unpack(names,1,n));assert(a==41 and b==43)
      assert(held.value==73);resumed=resumed+1
      if repeated then local c,d=coroutine.yield('again',79);assert(c==83 and d==89);resumed=resumed+1 end
      return'done'
    end})
    local a=pack(boundary_execution_concat_panic(co,message,x,'x'))
    assert(a[1]==0 and a[9]==n-2 and a[10]==1 and exported()==71)
    collectgarbage();assert(exported()==71)
    assert(debug.setupvalue(exported,1,{value=73})=='held')
    local result=pack(coroutine.resume(co,41,43))
    if repeated then
      assert(result.n==3 and result[1]and result[2]=='again'and result[3]==79)
      collectgarbage();result=pack(coroutine.resume(co,83,89))
    end
    assert(result.n==4 and result[1]and result[2]==names[n]and result[3]=='x'and result[4]=='done')
    assert(resumed==(repeated and 2 or 1)and coroutine.status(co)=='dead'and exported()==73)
    print('concat-resume',n,repeated,resumed,unpack(result,1,result.n));count=count+1
    collectgarbage()
  end
end
assert(count==40)
print('concat-yields-complete',count)
