local count=0
local function make(kind)
  local co=coroutine.create(function()
    if kind=='failed'then error('old')end
    if kind=='yielded'then coroutine.yield()end
  end)
  if kind~='fresh'then coroutine.resume(co)end
  return co
end
for _,kind in ipairs{'fresh','completed','yielded','failed'}do
  for _,visit in ipairs{1,2}do
    local original=getfenv(coroutine.yield)
    debug.setfenv(coroutine.yield,{marker=37})
    local co=make(kind);local globals={marker=31};debug.setfenv(co,globals)
    local environments={}
    local s,b,a,old,new,n,result=boundary_thread_load(co,'return marker',function(i)
      if i==visit then coroutine.yield(29)end
    end,'=reader-environment',3,environments)
    debug.setfenv(coroutine.yield,original)
    assert(#environments==n-(s==0 and 0 or 1))
    if kind=='fresh'then assert(s==2)else
      assert(s==0)
      if a>0 then assert(getfenv(result)==globals and result()==31)else assert(result==nil)end
    end
    local trace={}
    for i,environment in ipairs(environments)do
      local wanted
      if kind=='yielded'or kind~='fresh'and i>=visit then wanted=37
      elseif kind~='failed'then wanted=31 end
      assert(environment.marker==wanted,kind..' environment '..i..': '..tostring(environment.marker)..' ~= '..tostring(wanted))
      trace[i]=tostring(environment.marker)
    end
    print('reader-environment',kind,visit,s,b,a,old,new,n,table.concat(trace,','));count=count+1
    collectgarbage();collectgarbage()
  end
end
print('reader-environments-complete',count)
