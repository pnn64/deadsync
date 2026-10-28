do local co=coroutine.create(function()boundary_state()end);assert(coroutine.resume(co))end
collectgarbage()
local original=_G
local install=boundary_context_install
debug.setfenv(install,{label='installer'})
local function child(replacement,token)
  return coroutine.create(function(parent,native)
    local env,globals,capture=boundary_context_read(parent,native)
    boundary_context_write(parent,replacement,native,token)
    collectgarbage()
    return env,globals,capture
  end)
end
local function report(label,log,globals)
  print(label,#log)
  for _,r in ipairs(log) do
    print(r.event,r.kind,r.environment,r.userdata_environment,r.closure_environment,
      r.captures,r.capture_bound,r.globals==globals,
      debug.getfenv(r.userdata)==r.expected,debug.getfenv(r.closure)==r.expected,
      r.child_status,r.parent_environment==r.expected,r.parent_globals==globals,r.parent_capture,
      r.parent_environment_write,r.parent_capture_write)
  end
end
local before={context=1,label='native-before'}
local after={context=2,label='native-after'}
local f=boundary_context_make(17,'capture-two')
debug.setfenv(f,before)
local log={}
local token={}
install(log,after,child(after,token),token)
local a,b=f()
debug.sethook()
print('native-result',a,b,debug.getfenv(f)==after)
collectgarbage();report('native-context',log,original)
before={context=3,answer=17,label='lua-before'}
after={context=4,answer=42,label='lua-after'}
f=assert(loadstring('return answer','=native-context-lua'));setfenv(f,before)
log={};token={};install(log,after,child(after,token),token)
a=f()
debug.sethook()
print('lua-result',a,getfenv(f)==after)
collectgarbage();report('lua-context',log,original)
local globals={label='thread-globals'}
f=assert(loadstring('return answer','=native-context-thread'));setfenv(f,before)
log={};token={};install(log,after,child(after,token),token)
local co=coroutine.create(f);debug.setfenv(co,globals)
local ok,n=coroutine.resume(co)
debug.sethook()
print('thread-result',ok,n,getfenv(f)==after,debug.getfenv(co)==globals)
collectgarbage();report('thread-context',log,globals)
print('native-context-complete')
