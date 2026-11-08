local info,loc=boundary_debug_info,boundary_debug_local
local untouched=0x5a5a5a5a
local function emit(tag,r)
  assert(r.event==untouched)
  local bytes={}
  for i=1,60 do bytes[i]=string.format('%02x',r.short_bytes:byte(i)) end
  local lines={}
  for k,v in pairs(r.lines or {}) do assert(v==true);lines[#lines+1]=k end
  table.sort(lines)
  print(tag,r.found,r.status,r.delta,r.ci,r.line,r.nups,r.first,r.last,
    r.name,r.namewhat,r.what,r.source,table.concat(bytes),type(r['function']),table.concat(lines,','))
end
for _,options in ipairs{'','S','l','u','n','f','L','LSnulf','ffLL','?SfL','f?L','S\0?fL'} do
  emit('current-'..options,info(nil,0,options))
  emit('parent-'..options,info(nil,1,options))
  emit('tail-'..options,info(nil,-1,options))
end
emit('absent',info(nil,999,'SlnufL'))
local capture=41
local function target(a)
  local x=capture+a
  return x
end
for _,options in ipairs{'>','>SlnufL','>Lf','>ffLL','>?SufL','>S\0?fL'} do
  local r=info(nil,target,options)
  if options:find('f',1,true) and not options:find('\0',1,true) then assert(r['function']==target) end
  emit('function-'..options,r)
end
emit('c-function',info(nil,info,'>SlnufL'))
emit('binary-function',info(nil,assert(loadstring(string.dump(target))),'>SlnufL'))
for _,source in ipairs{'=short','@short/file.lua','return 1','='..string.rep('a',100),'@'..string.rep('b',100),'first\nsecond'} do
  emit('source',info(nil,assert(loadstring('return 1',source)),'>S'))
end
local function caller()
  local value=17
  local name,old,delta=loc(nil,1,1,false)
  assert(name=='value' and old==17 and delta==1)
  local name,_,delta=loc(nil,1,1,true,29)
  assert(name=='value' and delta==-1 and value==29)
  print('caller-local',name,value,delta)
  local name,_,delta=loc(nil,1,-1,true,99)
  assert(name==nil and delta==-1)
end
caller()
for _,n in ipairs{-1,0,1,3,5,6,7,999} do
  local name,value,delta,ci=loc(nil,0,n,false)
  print('c-local',n,name,type(value),delta,ci)
  local name,value,delta,ci=loc(nil,0,n,true,37)
  print('c-setlocal',n,name,type(value),delta,ci)
end
local function tail(n)
  if n>0 then return tail(n-1) end
  emit('lost-tail',info(nil,2,'?fL'))
end
tail(3)
local co=coroutine.create(function(a)
  local retained=a
  coroutine.yield(retained)
  return retained
end)
emit('fresh',info(co,0,'SlnufL'))
assert(coroutine.resume(co,37))
for level=-1,3 do emit('suspended',info(co,level,'SlnufL')) end
local name,old,delta=loc(co,1,2,false)
assert(name=='retained' and old==37 and delta==1)
assert(loc(co,1,2,true,73)=='retained')
assert(select(2,coroutine.resume(co))==73)
emit('finished',info(co,0,'SlnufL'))
co=coroutine.create(function()local a='held';error('fault',0) end)
assert(not coroutine.resume(co))
for level=0,3 do emit('failed',info(co,level,'SlnufL')) end
local parent
parent=coroutine.create(function()
  local retained=17
  local child=coroutine.create(function()
    for level=0,2 do emit('normal-parent',info(parent,level,'SlnufL')) end
    local name,value=loc(parent,1,1,false)
    assert(name=='retained' and value==17)
    assert(loc(parent,1,1,true,29)=='retained')
  end)
  assert(coroutine.resume(child));assert(retained==29)
end)
assert(coroutine.resume(parent))
co=coroutine.create(function(...)return boundary_yield(...)end)
boundary_thread_push(co,17,29)
assert(boundary_resume(co,2)==1)
emit('native-yield',info(co,0,'SlnufL'))
local name,value,delta=loc(co,0,1,false)
assert(name=='(*temporary)' and value==17 and delta==1)
assert(loc(co,0,1,true,37)=='(*temporary)')
assert(boundary_resume(co,0)==0)
for i=1,1000 do
  local f=assert(loadstring('return '..i,'=debug-'..i))
  local ok,source=boundary_debug_hold(f,function()collectgarbage('collect')end)
  assert(ok and source=='=debug-'..i)
end
print('native-debug-complete',1000)
