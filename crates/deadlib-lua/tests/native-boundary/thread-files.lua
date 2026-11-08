local prefix=THREAD_FILE_PREFIX or 'thread-file'
local escaped=prefix:gsub('(%W)','%%%1')
local function normalized(s)return(s:gsub(escaped,'FILE'))end
local binary=string.dump(assert(loadstring('return marker,41','=thread-file-binary')))
local entries={
  {'empty','',0},
  {'text','return marker,37',0},
  {'shebang','#! /lua\r\nreturn marker,37',0},
  {'binary',binary,0},
  {'shebang-binary','#! /lua\n'..binary,0},
  {'syntax','return function(',3},
  {'bad-binary','\027Lua\000broken',3},
  {'missing',nil,6},
  {'directory',nil,6},
}
local function path(name)return name=='directory'and(prefix..'-directory')or(prefix..'-'..name..'.lua')end
for _,e in ipairs(entries)do
  if e[2]then local f=assert(io.open(path(e[1]),'wb'));assert(f:write(e[2]));assert(f:close())end
end
local function make(kind)
  local co=coroutine.create(function()
    if kind=='failed'then error('old')end
    if kind=='yielded'then coroutine.yield(17)end
    if kind=='partial'then return boundary_thread_yield_tail(11,13,17)end
    return 23
  end)
  if kind~='fresh'then local ok=coroutine.resume(co);assert(ok==(kind~='failed'))end
  return co
end
local count=0
local function check(co,name,expected,nul)
  local env=setmetatable({marker=31},{__index=_G});debug.setfenv(co,env)
  local s,b,a,old,new,result,slots=boundary_thread_file(co,path(name)..(nul and'\000ignored'or''))
  assert(s==expected and old==new and a>=1 and slots[a]==result)
  local info=''
  if s==0 then
    assert(type(result)=='function'and getfenv(result)==env)
    collectgarbage();local x,y=result()
    if name=='empty'then assert(x==nil and y==nil)
    else assert(x==31 and y==((name=='binary'or name=='shebang-binary')and 41 or 37))end
    local source=debug.getinfo(result,'S').source
    assert(source==((name=='binary'or name=='shebang-binary')and'=thread-file-binary'or'@'..path(name)))
    info=normalized(source)
  else assert(type(result)=='string');info=normalized(result)end
  local types={};for i=1,a do types[i]=type(slots[i])end
  print(name,nul or false,s,b,a,old,new,type(result),table.concat(types,','),info)
  count=count+1
end
for _,kind in ipairs{'fresh','completed','yielded','failed','partial'}do
  for _,e in ipairs(entries)do
    local co=make(kind);check(co,e[1],e[3]);assert(coroutine.status(co)==((kind=='completed'or kind=='failed')and'dead'or'suspended'))
    if kind=='fresh'or kind=='yielded'then local ok,v=coroutine.resume(co);assert(ok and v==23)end
  end
  check(make(kind),'text',0,true)
end
do
  local parent=boundary_thread_called()
  local co=coroutine.create(function()
    check(parent,'text',0);check(parent,'syntax',3);check(parent,'missing',6)
  end)
  local ok,e=coroutine.resume(co);assert(ok,e)
end
for _,e in ipairs(entries)do if e[2]then assert(os.remove(path(e[1])))end end
print('thread-files-complete',count)
