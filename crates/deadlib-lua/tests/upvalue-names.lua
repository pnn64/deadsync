-- Imported chunks may keep capture cells while omitting some debug names.
local function observe(get)
  local out={debug.getinfo(get,'u').nups}
  local function record(...)
    out[#out+1]=select('#',...)
    for i=1,select('#',...)do out[#out+1]=tostring(select(i,...))end
  end
  for i=-1,4 do
    record(debug.getupvalue(get,i))
    record(debug.setupvalue(get,i,i*100))
    collectgarbage()
    record(debug.getupvalue(get,i))
  end
  record(get())
  local restored=assert(loadstring(string.dump(get)))
  record(debug.getinfo(restored,'u').nups)
  for i=1,2 do
    record(debug.getupvalue(restored,i))
    record(debug.setupvalue(restored,i,500+i))
  end
  record(restored())
  return table.concat(out,'|')
end

do
  local a,b=17,29
  local get=function()return a,b end
  local observation=observe(get)
  print('open',observation,a,b)
end

do
  local function factory()
    local a,b=17,29
    return function()return a,b end
  end
  print('closed',observe(factory()))
end

do
  local get
  local co=coroutine.create(function()
    local a,b=17,29
    get=function()return a,b end
    coroutine.yield()
    return a,b
  end)
  assert(coroutine.resume(co))
  local observation=observe(get)
  local ok,a,b=coroutine.resume(co)
  assert(ok)
  print('suspended',observation,a,b)
end
do
  local a,b=nil,17
  local functions={
    function()return a+b end,
    function()return a(b)end,
    function()return a[b]end,
    function()a[b]=1;return b end,
    function()return #a,b end,
    function()return -a,b end,
  }
  for i,f in ipairs(functions)do
    local ok,e=pcall(f)
    assert(not ok)
    print('operand-error',i,e,select('#',debug.getupvalue(f,2)))
  end
end
print('upvalue-names-complete')
