do local co=coroutine.create(function()boundary_state()end);assert(coroutine.resume(co))end
collectgarbage()
local function implementation(fn)if BODY_COPY then return boundary_body_copy(fn)end;return fn end
local load=implementation(package.loadlib)
local require_module=implementation(require)
package.loaders[3]=implementation(package.loaders[3]);package.loaders[4]=implementation(package.loaders[4])
local registry=debug.getregistry()
local function bytes(s)return(s:gsub('.',function(c)return('%02x'):format(c:byte())end))end
local function failure(path,symbol)
 local fn,message,where=load(path,symbol)
 assert(fn==nil and type(message)=='string');print('load-error',path,symbol,bytes(message),where)
 local ud=registry['LOADLIB: '..path];assert(type(ud)=='userdata'and getmetatable(ud)==registry._LOADLIB)
 print('handle',boundary_library_handle(ud),boundary_length(ud))
 return ud
end
failure('./absent-native.dll','symbol')
failure('./invalid-native.dll','symbol')
local f=assert(load('./native.dll','boundary_sum'))
local again=assert(load('./native.dll','boundary_sum'))
assert(f(17,25)==42 and again(29,13)==42 and f~=again)
local exists,same,other=boundary_body_info(f,again)
assert(exists and same and other);print('symbols',exists,same,other)
local ud=registry['LOADLIB: ./native.dll']
assert(boundary_library_handle(ud));assert(debug.getfenv(ud)==debug.getfenv(load))
failure('./native.dll','missing_symbol')
assert(load('./native.dll\0ignored','boundary_sum\0ignored')(17,25)==42)
local old_environment=debug.getfenv(load)
local environment={token={}}
debug.setfenv(load,environment)
local init=assert(load('./native.dll','luaopen_native_echo'))
assert(debug.getfenv(init)==environment)
local module=init('direct')
assert(module.name=='direct'and module.environment==environment and module.token==environment.token)
assert(debug.getfenv(module.sum)==environment and module.sum(17,25)==42)
debug.setfenv(load,old_environment)
print('environment',true)
package.path='';package.cpath='./?.dll'
local a=require_module('native.root.leaf')
assert(a.name=='native.root.leaf'and a.environment==package and a.sum(29,13)==42)
assert(require_module('native.root.leaf')==a)
local calls=registry['native.library.calls'];assert(calls==2)
local ok,missing=pcall(require_module,'native.root.missing')
assert(not ok and missing:find("no module 'native.root.missing' in file './native.dll'",1,true))
print('require-root',calls,true)
package.cpath='./native.dll'
assert(require_module('native.nothing')==true)
assert(require_module('native.false')==false)
assert(require_module('native.false')==false)
local tag={};registry['native.library.error']=tag
local ok,err=pcall(require_module,'native.error');assert(not ok and err==tag)
assert(type(package.loaded['native.error'])=='userdata')
print('module-results',true)
-- Return hooks must see the registry handle and alias the pending function.
for _,symbol in ipairs{'boundary_sum','missing_symbol'}do
 for _,slot in ipairs{0,1,-1}do
  local log={};local co=coroutine.create(function()return load('./native.dll',symbol,'extra')end)
  boundary_return_stack(load,log,co,slot,'hook-token')
  local results={coroutine.resume(co)}
  assert(results[1]and #log==1)
  if slot==-1 then assert(results[symbol=='boundary_sum'and 2 or 4]=='hook-token')end
  local row=log[1];local values={}
  for i=1,row.top do
   local v=row[i];local t=type(v)
   if t=='string'then values[#values+1]=t..':'..bytes(v)
   elseif t=='nil'then values[#values+1]='nil'
   else values[#values+1]=t end
  end
  print('load-stack',symbol,slot,row.top,table.concat(values,','),type(results[2]),results[2]=='hook-token')
 end
end
-- Removing cache entries permits collection; closed handles can be reopened.
registry._LOADLIB.__gc(ud);assert(not boundary_library_handle(ud))
registry._LOADLIB.__gc(ud)
local reopened=assert(load('./native.dll','boundary_sum'));assert(reopened(17,25)==42)
assert(registry['LOADLIB: ./native.dll']==ud and boundary_library_handle(ud))
local close=implementation(registry._LOADLIB.__gc)
close(ud);assert(not boundary_library_handle(ud))
assert(load('./native.dll','boundary_sum')(17,25)==42)
local close_mt=registry._LOADLIB
local old_registry_mt=debug.getmetatable(registry)
registry._LOADLIB=nil
local saved={};local log={}
setmetatable(registry,{
 __index=function(t,k)
  local fn=debug.getinfo(2,'f').func
  if k=='_LOADLIB'then
   assert(fn==close or fn==load)
   if fn==close then
    local _,mt=debug.getlocal(2,2);assert(mt==close_mt)
   end
   return close_mt
  end
  if k:sub(1,9)=='LOADLIB: 'then
   assert(fn==load);local _,key=debug.getlocal(2,3);assert(key==k)
   collectgarbage();return saved[k]
  end
 end,
 __newindex=function(t,k,v)
  assert(debug.getinfo(2,'f').func==load)
  local _,original=debug.getlocal(2,3);local _,key=debug.getlocal(2,4);local _,copy=debug.getlocal(2,5)
  assert(original==v and copy==v and key==k);saved[k]=v;collectgarbage()
 end
})
local alias=assert(load('native.dll','boundary_sum'));assert(alias(29,13)==42)
assert(load('native.dll','boundary_sum')(17,25)==42)
close(ud);assert(not boundary_library_handle(ud))
setmetatable(registry,old_registry_mt);registry._LOADLIB=close_mt
assert(load('./native.dll','boundary_sum')(17,25)==42)
print('registry-callbacks',true)
registry['LOADLIB: ./native.dll']=nil;ud=nil;f=nil;again=nil;reopened=nil;init=nil;module=nil
collectgarbage();collectgarbage()
print('native-library-complete')
