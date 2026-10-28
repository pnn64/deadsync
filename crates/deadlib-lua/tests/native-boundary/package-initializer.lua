-- Run through the public C entrypoint in a state without preopened libraries.
local bare=_G==nil and _VERSION==nil and assert==nil and math==nil and string==nil
  and table==nil and os==nil and debug==nil and coroutine==nil and io==nil and package==nil
local registry,registry_count=library_registry()
local bare_string=library_metatable('')==nil
local names={'base','table','string','math','os','debug','package'}
local opened={};local package_seen=false;local order=library_order
for i=1,7 do
  local index
  if order<=7 then index=(i+order-2)%7+1 else index=(order-i+5)%7+1 end
  local name=names[index]
  local n,s,a=library_open(name,'argument',nil,17)
  opened[name]={n=n,s=s,a=a,package_env=package_seen}
  if name=='package'then package_seen=true end
end
assert(bare and bare_string and registry_count==0 and io==nil)
local rows={}
local function emit(label,...)
  local row={label}
  for i=1,select('#',...)do row[#row+1]=tostring(select(i,...))end
  rows[#rows+1]=table.concat(row,'\t')
end
emit('bare',bare,bare_string,io==nil,registry_count==0)
local p=package;local mt=registry._LOADLIB
local v=opened.package
assert(v.n==1 and v.s.n==5 and v.s[1]=='argument'and v.s[2]==nil and v.s[3]==17)
assert(v.s[4]==mt and v.s[5]==p and v.a==p)
emit('package.stack',v.n,v.s.n,v.s[4]==mt,v.s[5]==p)
assert(p.loaded==registry._LOADED and p.loaded.package==p and p.loaded._G==_G)
assert(debug.getfenv(library_open)==p)
assert(debug.getfenv(p.loadlib)==_G and debug.getfenv(p.seeall)==_G and debug.getfenv(mt.__gc)==_G)
assert(debug.getfenv(require)==p and debug.getfenv(module)==p)
emit('package.environments',debug.getfenv(library_open)==p,debug.getfenv(p.loadlib)==_G,debug.getfenv(require)==p)
assert(#p.loaders==4 and type(p.preload)=='table'and next(p.preload)==nil and loadlib==nil)
for i=1,4 do
  assert(debug.getfenv(p.loaders[i])==p and debug.getinfo(p.loaders[i]).what=='C')
  emit('loader.'..i,debug.getfenv(p.loaders[i])==p,debug.getinfo(p.loaders[i]).what)
end
for _,name in ipairs{'base','table','string','math','os','debug'}do
  local field=({base='assert',table='concat',string='sub',math='abs',os='difftime',debug='getfenv'})[name]
  local lib=name=='base'and _G or _G[name]
  local env=opened[name].package_env and p or _G
  assert(debug.getfenv(lib[field])==env)
  emit('initial.environment.'..name,debug.getfenv(lib[field])==env)
end

local function expected_path(variable,fallback)
  local path=library_environment(variable)
  if path==nil then path=fallback
  else path=path:gsub(';;',';\1;'):gsub('\1',function()return fallback end)end
  local directory=library_program_directory()
  if directory then path=path:gsub('!',function()return directory end)end
  return path
end
local windows=library_program_directory()~=nil
local path_default=windows and '.\\?.lua;!\\lua\\?.lua;!\\lua\\?\\init.lua;!\\?.lua;!\\?\\init.lua'
  or './?.lua;/usr/local/share/lua/5.1/?.lua;/usr/local/share/lua/5.1/?/init.lua;/usr/local/lib/lua/5.1/?.lua;/usr/local/lib/lua/5.1/?/init.lua'
local cpath_default=windows and '.\\?.dll;!\\?.dll;!\\loadall.dll'
  or './?.so;/usr/local/lib/lua/5.1/?.so;/usr/local/lib/lua/5.1/loadall.so'
local expected_config=windows and '\\\n;\n?\n!\n-'or '/\n;\n?\n!\n-'
assert(p.path==expected_path('LUA_PATH',path_default))
assert(p.cpath==expected_path('LUA_CPATH',cpath_default)and p.config==expected_config)
emit('package.paths',p.path==expected_path('LUA_PATH',path_default),p.cpath==expected_path('LUA_CPATH',cpath_default),p.config==expected_config)

local count=0;local marker={}
p.preload.initializer_probe=function(name)
  count=count+1;collectgarbage();assert(name=='initializer_probe');return marker
end
assert(require('initializer_probe')==marker and require('initializer_probe')==marker and count==1)
emit('require.preload',count,require('initializer_probe')==marker)
p.preload.initializer_nil=function(name)collectgarbage();assert(name=='initializer_nil')end
assert(require('initializer_nil')==true and p.loaded.initializer_nil==true)
emit('require.nil',require('initializer_nil'),p.loaded.initializer_nil)
local function declare_module()
  module('initializer_module.child',package.seeall,function(t)collectgarbage();t.marker=17 end)
  return _M,_NAME,_PACKAGE,assert
end
local m,name,prefix,assertion=declare_module()
assert(m==initializer_module.child and m==p.loaded[name]and name=='initializer_module.child')
assert(m.marker==17 and prefix=='initializer_module.'and assertion==assert and m._M==m)
emit('module',name,prefix,m.marker,m._M==m,debug.getfenv(declare_module)==m)

-- Fresh loaders keep the package environment for Lua and C searches.
local module_path=library_module_path or library_environment('LUA_INITIALIZER_MODULE')
assert(type(module_path)=='string')
local entry=assert(p.loadlib(module_path,'luaopen_package_probe'))
local native=entry('direct')
assert(native.name=='direct'and native.environment==_G and debug.getfenv(native.identity)==_G)
assert(select('#',native.identity(17,nil,false))==3 and native.identity(17)==17)
emit('native.direct',native.name,native.environment==_G,debug.getfenv(native.identity)==_G)
p.path='';p.cpath=module_path
local imported=require('package_probe')
assert(imported.name=='package_probe'and imported.environment==p and require('package_probe')==imported)
local root=assert(p.loaders[4]('package_probe.child'))('package_probe.child')
assert(root.name=='package_probe.child'and root.environment==p)
emit('native.require',imported.name,imported.environment==p,root.name,root.environment==p)
p.cpath=''
assert(p.loaders[2]('missing')==''and p.loaders[3]('missing')=='')
assert(select('#',p.loaders[4]('missing'))==0 and p.loaders[4]('missing.child')=='')
local ok,err=pcall(require,'missing')
local expected_missing="module 'missing' not found:\n\tno field package.preload['missing']"
assert(not ok and err:sub(-#expected_missing)==expected_missing)
emit('require.missing',ok,err:sub(-#expected_missing)==expected_missing)

-- Reopening retains the loaded table but replaces functions, loaders and preload.
local oldloadlib,oldseeall,oldrequire,oldmodule=p.loadlib,p.seeall,require,module
local oldloaders,oldpreload,oldgc=p.loaders,p.preload,mt.__gc
local n,s,same=library_open('package',17,nil)
assert(n==1 and s.n==4 and s[3]==mt and s[4]==p and same==p and registry._LOADLIB==mt)
assert(p.loadlib~=oldloadlib and p.seeall~=oldseeall and require~=oldrequire and module~=oldmodule)
assert(p.loaders~=oldloaders and p.preload~=oldpreload and mt.__gc~=oldgc)
assert(p.loaded.initializer_probe==marker and p.loaded.package_probe==imported)
assert(debug.getfenv(p.loadlib)==p and debug.getfenv(mt.__gc)==p)
emit('package.reopen',same==p,p.loadlib~=oldloadlib,p.loaders~=oldloaders,p.preload~=oldpreload,mt.__gc~=oldgc)
assert(p.path==expected_path('LUA_PATH',path_default)and p.cpath==expected_path('LUA_CPATH',cpath_default))

local loaded=registry._LOADED
do
  local env={marker='caller'};debug.setfenv(library_open,env)
  local _,_,same=library_open('package')
  assert(same==p and debug.getfenv(p.loadlib)==env and debug.getfenv(p.seeall)==env)
  assert(debug.getfenv(mt.__gc)==env and debug.getfenv(p.loaders[1])==p and debug.getfenv(require)==p)
  assert(debug.getfenv(library_open)==p)
  emit('package.caller.environment',debug.getfenv(p.loadlib)==env,debug.getfenv(mt.__gc)==env,debug.getfenv(library_open)==p)
end
do
  local events={};local cached=setmetatable({},{__newindex=function(t,k,v)
    events[#events+1]=k;rawset(t,k,v)
  end})
  loaded.package=cached
  local _,_,value=library_open('package')
  assert(value==cached and package==p and cached.loaded==loaded and loaded.package==cached)
  assert(table.concat(events,',')=='loadlib,seeall,loaders,path,cpath,config,loaded,preload')
  assert(debug.getfenv(require)==cached and debug.getfenv(library_open)==cached)
  emit('package.cached',value==cached,package==p,table.concat(events,','))
  loaded.package=p;library_open('package')
end
do
  local marker={};local count=0;local partial=setmetatable({},{__newindex=function(t,k,v)
    count=count+1;if k=='config'then error(marker,0)end;rawset(t,k,v)
  end})
  loaded.package=partial
  local oldrequire=require;local ok,err=pcall(library_open,'package')
  assert(not ok and err==marker and count==6 and partial.cpath and partial.config==nil)
  assert(debug.getfenv(library_open)==partial and require==oldrequire)
  emit('package.partial',ok,err==marker,count,partial.config==nil,require==oldrequire)
  loaded.package=p;library_open('package')
end
do
  local oldgc=mt.__gc;package=17;loaded.package=nil
  local ok,err=pcall(library_open,'package')
  assert(not ok and err:match("name conflict for module 'package'$"))
  assert(mt.__gc~=oldgc)
  emit('package.conflict',ok,err:match('name conflict.*$'),mt.__gc~=oldgc)
  package=p;loaded.package=p;library_open('package')
end

-- ENVIRONINDEX needs an existing C activation on a selected target.
do
  local co=coroutine.create(function()coroutine.yield('suspended');return 'finished'end)
  assert(coroutine.resume(co))
  local env={marker='target'};debug.setfenv(co,env);loaded.package=nil
  local n,s,value=library_open_thread(co,'package','argument',nil,17)
  assert(n==1 and s.n==5 and s[4]==mt and s[5]==value)
  assert(value==env.package and loaded.package==value and package==p)
  assert(debug.getfenv(coroutine.yield)==value and debug.getfenv(env.require)==value)
  emit('package.thread.yielded',n,s.n,value==env.package,package==p,coroutine.status(co))
  loaded.package=p;debug.setfenv(coroutine.yield,_G)
  local ok,value=coroutine.resume(co);assert(ok and value=='finished')
end
do
  local parent,env
  local child=coroutine.create(function()
    local n,s,value=library_open_thread(parent,'package','argument',nil,17)
    assert(n==1 and s.n==5 and s[4]==mt and s[5]==value and value==env.package)
    assert(debug.getfenv(coroutine.resume)==value and debug.getfenv(env.require)==value)
    emit('package.thread.waiting',n,s.n,value==env.package,package==p,coroutine.status(parent))
    return 'child'
  end)
  parent=coroutine.create(function()return coroutine.resume(child)end)
  env={marker='waiting'};debug.setfenv(parent,env);loaded.package=nil
  local ok,child_ok,value=coroutine.resume(parent)
  assert(ok and child_ok and value=='child')
  loaded.package=p;debug.setfenv(coroutine.resume,_G)
end
library_open('package')
emit('complete',#rows+1)
return table.concat(rows,'\n')..'\n'
