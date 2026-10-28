do local co=coroutine.create(function()boundary_state()end);assert(coroutine.resume(co))end
collectgarbage()
local function pack(...)return {n=select('#',...),...}end
local empty=boundary_newthread()
for _,pair in ipairs{
 {math.abs,math.abs},{math.abs,math.sin},{io.close,getmetatable(io.stdout).__index.close},
 {next,(pairs({}))},{string.gmatch('1','%d'),string.gmatch('2','%d')},
 {coroutine.wrap(function()end),coroutine.wrap(function()end)},
 {boundary_identity,boundary_identity},{boundary_identity,boundary_sum},
 {function()end,function()end},{false,false},
}do print('pointer',boundary_body_info(pair[1],pair[2],empty))end
local copy=boundary_body_copy
local ab=copy(math.abs);assert(ab(-17)==17)
print('copied-pointer',boundary_body_info(ab,math.abs,empty))
local token={};local self
self=copy(pcall)
local events={}
debug.sethook(function(e)
 if debug.getinfo(2,'f').func==self then events[#events+1]=e end
end,'cr')
local function callback()
 assert(debug.getinfo(2,'f').func==self);collectgarbage();return token
end
local ok,v=self(callback);debug.sethook();assert(ok and v==token)
print('visible',table.concat(events,','))
local xp=copy(xpcall);local handled={}
local ok,e=xp(function()error(token,0)end,function(e)assert(e==token);return handled end)
assert(not ok and e==handled);print('protected',ok,e==handled)
local resume=copy(coroutine.resume)
local co=coroutine.create(function(x)coroutine.yield(x);return 29 end)
local ok,x=resume(co,17);assert(ok and x==17);print('resume',ok,x)
ok,x=resume(co);assert(ok and x==29);print('resume-done',ok,x)
local wrap=copy(coroutine.wrap(function(x)coroutine.yield(x);return 42 end))
assert(wrap(17)==17 and wrap()==42);print('wrap',coroutine.status(co))
local yield=copy(coroutine.yield)
co=coroutine.create(function()return yield(17,29)end)
local ok,a,b=resume(co);assert(ok and a==17 and b==29);print('yield',ok,a,b)
ok,a,b=resume(co,42,57);assert(ok and a==42 and b==57);print('yield-return',ok,a,b)
assert(boundary_body_call(math.abs,-17)==17)
local close=getmetatable(io.stdout).__index.close
local ok,a=pcall(close);local copied_ok,b=pcall(copy(close))
assert(not ok and not copied_ok and a:match('FILE%* expected, got nil')and b:match('FILE%* expected, got nil'))
print('close-alias',ok,copied_ok)
local original=io.output();local fake={};local output=copy(io.output)
local env={[2]=fake};debug.setfenv(output,env);assert(output()==fake and io.output()==original)
print('environment',output()==fake)
local env={};local target=setfenv(function(x)return x end,env)
for _,fn in ipairs{target,boundary_identity}do
 for mode=0,1 do
  local log={};local expected=mode==0 and 17 or(fn==target and env or _G)
  local co=coroutine.create(function()return fn(-17)end)
  boundary_body_hook(co,fn,mode==0 and math.abs or getfenv,mode,expected,log)
  local ok,v=coroutine.resume(co);assert(ok and v==-17)
  assert(log.same and log.result and log.count==1)
  print('hook-context',fn==target,mode,log.before,log.after,log.count,log.same,log.result)
 end
end
print('native-bodies-complete')
local freed=false;local p=newproxy(true);getmetatable(p).__gc=function()freed=true end
debug.setfenv(tostring,{p=p});p=nil
tostring=copy(tostring,true)
local value=setmetatable({},{__tostring=function()collectgarbage();return 'ok'end})
assert(tostring(value)=='ok'and freed)
print('body-code-lifetime',freed)
