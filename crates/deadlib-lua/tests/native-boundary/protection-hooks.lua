do local co=coroutine.create(function()boundary_state()end);assert(coroutine.resume(co))end
collectgarbage()
local function pack(...)return {n=select('#',...),...}end
local token,handled={},{}
local filename='protection-hooks-input.lua'
local badfilename='protection-hooks-invalid.lua'
local file=assert(io.open(filename,'wb'));file:write('return 17');file:close()
file=assert(io.open(badfilename,'wb'));file:write('return +');file:close()
local tests={
 {'pcall',pcall,function()return pcall(function()return 17 end)end},
 {'pcall-error',pcall,function()return pcall(function()error(token,0)end)end},
 {'xpcall',xpcall,function()return xpcall(function()return 17 end,function()error('expired inner handler',0)end)end},
 {'xpcall-error',xpcall,function()return xpcall(function()error(token,0)end,function()return handled end)end},
 {'load',load,function()local i=0;return load(function()i=i+1;if i==1 then return 'return 17'end end)end},
 {'load-error',load,function()return load(function()return 'return +'end)end},
 {'load-reader-error',load,function()return load(function()error(token,0)end)end},
 {'loadstring',loadstring,function()return loadstring('return 17')end},
 {'loadstring-error',loadstring,function()return loadstring('return +')end},
 {'loadfile',loadfile,function()return loadfile(filename)end},
 {'loadfile-error',loadfile,function()return loadfile(badfilename)end},
 {'loadfile-open',loadfile,function()return loadfile('absent-protection-hooks-file')end},
 {'dofile',dofile,function()return dofile(filename)end},
}
local count=0
for _,case in ipairs(tests)do
 for mode=1,3 do
  local log={};local handler_calls,local_calls=0,0
  local co=coroutine.create(function()
   local hook_action=true
   if mode==3 then
    hook_action=function()
     debug.sethook()
     local ok,e=pcall(function()error(token,0)end);assert(not ok and e==token);local_calls=local_calls+1
     local fn,err=load(function()error(token,0)end)
     assert(fn==nil and err==handled)
     collectgarbage();error(token,0)
    end
   end
   boundary_return_stack(case[2],log,nil,0,token,hook_action)
   if mode==1 then return pcall(case[3])end
   return xpcall(case[3],function(e)handler_calls=handler_calls+1;assert(e==token);return handled end)
  end)
  local results=pack(coroutine.resume(co));count=count+1
  print('protection',case[1],mode,#log,handler_calls,local_calls,results.n)
  for i=1,results.n do
   local v=results[i]
   print('result',i,type(v),v==token,v==handled,type(v)=='boolean'and tostring(v)or'')
  end
 end
end
assert(os.remove(filename));assert(os.remove(badfilename))
print('protection-hooks-complete',count)
