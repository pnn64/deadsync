do local co=coroutine.create(function()boundary_state()end);assert(coroutine.resume(co))end
collectgarbage()
local function report(label,log)
  print(label,#log)
  for i,row in ipairs(log) do
    print(i,row.event,row.line,row.ci,row.top,row.mask,row.count,row.same,row.stack,
      row.currentline,row.first,row.what,row.name,row.current_ci,row.second,row.alias,row.level)
  end
end
local co=coroutine.create(function()end)
for _,pair in ipairs{{256,-3},{-1,0},{8,-17},{0,29},{4,-1},{1,17}} do
  assert(boundary_hook_set(co,pair[1],pair[2],1,{})==1)
  print('controls',unpack(pair));print(boundary_hook_info(co));print(debug.gethook(co))
end
assert(boundary_hook_set(co,7,-17,0,{})==1)
print('disabled',boundary_hook_info(co))
local target=assert(loadstring('local x=...;local function add(n)return n+1 end\nlocal sum=0\nfor i=1,3 do sum=sum+add(i) end\nreturn sum,x', '=hook-target'))
for _,mask in ipairs{1,2,4,8,15} do
  local log={}
  boundary_hook_set(nil,mask,3,1,log)
  local a,b=target(17)
  debug.sethook()
  assert(a==9 and b==17)
  report('events-'..mask,log)
end
log={}
boundary_hook_set(nil,3,0,1,log)
assert(boundary_identity(17)==17)
debug.sethook();report('native-target',log)
log={}
boundary_hook_set(nil,2,0,1,log)
for _,entry in ipairs{{math.abs,-17},{math.modf,17.5},{math.frexp,17},{math.ldexp,0.5,3},{math.min,3,1,2},{math.randomseed,17}} do
  entry[1](unpack(entry,2))
end
debug.sethook();report('math-return-windows',log)
log={}
boundary_hook_set(nil,2,0,7,log)
local changed=math.abs(-17)
debug.sethook();assert(changed==42);report('math-return-mutation',log)
local log={}
boundary_hook_set(nil,1,0,2,log)
local f=assert(loadstring('return function(x)return x end','=hook-mutate'))()
assert(f(17)==29)
debug.sethook();report('mutation',log)
local token={}
f=assert(loadstring('local x=...;return x','=hook-error'))
local function invoke()boundary_hook_set(nil,1,0,3,{},token);return f(token)end
local ok,err=pcall(invoke);assert(not ok and err==token)
print('hook-error',ok,err==token)
local function luahook()end
debug.sethook(luahook,'c',7)
boundary_hook_restore(nil,true)
boundary_hook_set(nil,4,0,1,{})
boundary_hook_restore(nil,false,1,7)
assert(debug.gethook()==luahook)
print('restored-lua',boundary_hook_info())
debug.sethook()
log={}
boundary_hook_set(nil,1,0,1,log)
co=coroutine.create(function(x)return x end)
local h,m,c=debug.gethook(co);assert(h=='external hook' and m=='c' and c==0)
local ok,n=coroutine.resume(co,29);assert(ok and n==29)
debug.sethook();report('inherited',log)
log={}
boundary_hook_set(nil,12,1,4,log)
target(17)
debug.sethook();report('replacement',log)
for _,mask in ipairs{4,8,12} do
  log={}
  co=coroutine.create(assert(loadstring('local x=...\nlocal y=x+1\nreturn y','=hook-yield')))
  boundary_hook_set(co,mask,1,5,log)
  local results={coroutine.resume(co,17)}
  print('yield',mask,#results,unpack(results))
  print('suspended',coroutine.status(co),boundary_thread_status(co))
  assert(results[1] and #results==1 and coroutine.status(co)=='suspended')
  local ok,n=coroutine.resume(co,99);assert(ok and n==18)
  report('yield-events-'..mask,log)
end
log={}
local calls=0
local levels={}
local main=boundary_thread()
target=assert(loadstring('local x=...\nlocal y=x+1\nreturn y','=hook-reentry'))
boundary_hook_set(nil,4,0,6,log,function()calls=calls+1;levels[#levels+1]=boundary_level(main);collectgarbage()end)
assert(target(17)==18)
debug.sethook();report('reentry',log)
print('reentry-calls',calls)
print('reentry-levels',unpack(levels))
print('native-hooks-complete')
