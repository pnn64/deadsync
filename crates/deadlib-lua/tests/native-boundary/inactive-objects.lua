local probe=boundary_inactive_objects
local count=0
local function check(label,co,expected)
  local meta={__index=function()error('raw __index ran')end,
    __newindex=function()error('raw __newindex ran')end,__metatable='hidden'}
  local token={label=label}
  local cell=101
  local function source()return cell end
  local status=coroutine.status(co)
  local t,u,c,f,s=probe(co,meta,token,source,expected or _G)
  assert(coroutine.status(co)==status)
  assert(t.a==17 and t[1]==25 and t[2]==11 and debug.getmetatable(t)==meta)
  assert(type(u)=='userdata' and debug.getfenv(u)==token)
  assert(f==source and f()==103 and cell==103)
  local a,b,env=c();assert(a==29 and b==token and env==token)
  assert(s[1]==3 and s[2]==42 and s[5]==57 and s[6])
  collectgarbage();collectgarbage()
  a,b,env=c();assert(a==29 and b==token and env==token and f()==103)
  print(label,status,s[1],s[2],s[3],s[4],s[5],s[6],type(t),type(u),a,b==token,env==token)
  count=count+1
end
for i=1,4 do
  local co=coroutine.create(function()return 'done'end)
  local globals={label='fresh-globals'};debug.setfenv(co,globals)
  check('fresh',co,globals);local ok,value=coroutine.resume(co);assert(ok and value=='done')
  check('completed',co,globals)
  co=coroutine.create(function()coroutine.yield(17,'yielded');return 29 end)
  assert(coroutine.resume(co));check('yielded',co)
  local ok,value=coroutine.resume(co);assert(ok and value==29)
  co=coroutine.create(function()error('inactive failure')end)
  assert(not coroutine.resume(co));check('failed',co)
end
local resume=coroutine.resume
local resume_environment=debug.getfenv(resume)
local waiting_environment={label='waiting-C-environment'}
debug.setfenv(resume,waiting_environment)
local function wait(co)
  local child=coroutine.create(function()check('waiting',co,waiting_environment)end)
  assert(coroutine.resume(child))
end
local co=coroutine.create(function()wait(coroutine.running())end)
assert(coroutine.resume(co))
debug.setfenv(resume,resume_environment)
print('inactive-objects-complete',count)
