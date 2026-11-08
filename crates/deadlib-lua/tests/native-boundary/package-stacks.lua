do local co=coroutine.create(function()boundary_state()end);assert(coroutine.resume(co))end
collectgarbage()
local function pack(...)return {n=select('#',...),...}end
local labels=setmetatable({},{__mode='k'})
local function mark(value,label)labels[value]=label;return value end
mark(_G,'globals');mark(debug.getregistry(),'registry');mark(package,'package')
mark(package.loaded,'loaded');mark(package.preload,'preload');mark(package.loaders,'loaders')
local function encode(value)
  local t=type(value)
  if t=='nil'or t=='boolean'or t=='number'then return tostring(value)end
  if t=='string'then return(value:gsub('.',function(c)return('%02x'):format(c:byte())end))end
  if labels[value]then return t..':'..labels[value]end
  if t=='function'then local d=debug.getinfo(value,'Su');return 'function:'..d.what..':'..d.nups end
  return t
end
local count=0
local function observe(name,fn,args,index)
  if BODY_COPY then fn=boundary_body_copy(fn)end
  count=count+1
  local log={};local co=coroutine.create(function()return fn(unpack(args,1,args.n))end)
  boundary_return_stack(fn,log,co,index,'hook-token')
  local results=pack(coroutine.resume(co))
  print('CASE',name..':'..index,#log,results.n)
  for i=1,results.n do print('RESULT',i,encode(results[i]))end
  for _,row in ipairs(log)do
    print('TOP',row.top)
    for i=1,row.top do print('SLOT',i,encode(row[i]))end
  end
end
local function probe(name,make)
  for _,index in ipairs{0,1,-1}do
    local fn,args,cleanup=make();observe(name,fn,args,index)
    if cleanup then cleanup()end
  end
end
local tag=mark({},'tag')
local original_loaders=package.loaders
local original_path,original_cpath=package.path,package.cpath
local original_preload=package.preload
for _,value in ipairs{17,'cached',tag,true}do
  probe('require-cached-'..type(value),function()
    package.loaded.stack_cached=value
    return require,pack('stack_cached',tag),function()package.loaded.stack_cached=nil end
  end)
end
for _,kind in ipairs{'value','nil','false','set','clear','set-false'}do
  probe('require-preload-'..kind,function()
    package.loaded.stack_preload=nil
    package.preload.stack_preload=function(name)
      assert(name=='stack_preload');collectgarbage()
      if kind=='value'then return tag end
      if kind=='false'then return false end
      if kind=='set'then package.loaded[name]=tag end
      if kind=='clear'then package.loaded[name]=nil end
      if kind=='set-false'then package.loaded[name]=false end
    end
    return require,pack('stack_preload',tag),function()
      package.preload.stack_preload=nil;package.loaded.stack_preload=nil
    end
  end)
end
probe('require-custom-loaders',function()
  package.loaded.stack_custom=nil
  package.loaders=mark({function()return 'first:'end,function()return 17 end,
    function()return false end,function()return function()return tag end end},'custom-loaders')
  return require,pack('stack_custom',tag),function()
    package.loaders=original_loaders;package.loaded.stack_custom=nil
  end
end)
probe('require-numeric-name',function()
  package.loaded['12345']=nil
  package.preload['12345']=function(name)assert(name=='12345');return 29 end
  return require,pack(12345,tag),function()package.loaded['12345']=nil;package.preload['12345']=nil end
end)
probe('require-nul-name',function()
  package.loaded.stack_nul=nil;package.preload.stack_nul=function()return tag end
  return require,pack('stack_nul\0suffix',tag),function()package.loaded.stack_nul=nil;package.preload.stack_nul=nil end
end)
probe('require-initial-false',function()
  package.loaded.stack_false=false;package.preload.stack_false=function()end
  return require,pack('stack_false',tag),function()package.loaded.stack_false=nil;package.preload.stack_false=nil end
end)
probe('require-mutated-slots',function()
  package.loaded.stack_mutation=nil
  local target=mark({},'replacement-loaded')
  local loaders=mark({},'replacement-loaders')
  package.loaders={function()
    assert(debug.setlocal(2,1,'changed-argument'));assert(debug.setlocal(2,2,target))
    assert(debug.setlocal(2,3,tag));assert(debug.setlocal(2,4,loaders))
    assert(debug.setlocal(2,5,'changed-accumulator'));collectgarbage()
    return function()end
  end}
  return require,pack('stack_mutation',tag),function()
    package.loaders=original_loaders;package.loaded.stack_mutation=nil
  end
end)
local loader=mark(function()end,'preloaded-function')
for _,value in ipairs{loader,17,false,tag}do
  probe('preload-found-'..type(value),function()
    package.preload.stack_lookup=value
    return original_loaders[1],pack('stack_lookup',tag),function()package.preload.stack_lookup=nil end
  end)
end
probe('preload-missing',function()return original_loaders[1],pack('stack_missing',tag)end)
probe('preload-numeric',function()
  package.preload['12345']=loader
  return original_loaders[1],pack(12345,tag),function()package.preload['12345']=nil end
end)
probe('preload-index',function()
  package.preload=mark(setmetatable({},{__index=function()collectgarbage();return loader end}),'indexed-preload')
  return original_loaders[1],pack('stack_index',tag),function()package.preload=original_preload end
end)
for _,kind in ipairs{'new','global','loaded','initialized','options','no-options','nul'}do
  probe('module-'..kind,function()
    local name=kind=='nul'and'stack_module\0suffix'or'stack_module'
    if kind=='global'then _G.stack_module=mark({},'module')end
    if kind=='loaded'then package.loaded.stack_module=mark({},'module')end
    if kind=='initialized'then package.loaded.stack_module=mark({_NAME='already'},'module')end
    local args
    if kind=='options'then args=pack(name,package.seeall,function(m)assert(m._M==m);collectgarbage()end)
    elseif kind=='no-options'then args=pack(name)
    else args=pack(name,function(m)mark(m,'module')end)end
    return module,args,function()_G.stack_module=nil;package.loaded.stack_module=nil end
  end)
end
probe('module-dotted',function()
  return module,pack('stack_parent.child',function(m)mark(m,'module')end),function()
    _G.stack_parent=nil;package.loaded['stack_parent.child']=nil
  end
end)
probe('module-numeric',function()
  return module,pack(12345,function(m)mark(m,'module')end),function()
    _G['12345']=nil;package.loaded['12345']=nil
  end
end)
probe('module-no-return',function()
  return module,pack('stack_module',function()end),function()_G.stack_module=nil;package.loaded.stack_module=nil end
end)
for _,existing in ipairs{false,true}do
  probe('seeall-'..tostring(existing),function()
    local mt=mark({},'module-metatable');local t=mark({},'module')
    if existing then setmetatable(t,mt)end
    return package.seeall,pack(t,tag)
  end)
end
probe('seeall-changing-metatable',function()
  local old=mark({},'original-metatable');local replacement=mark({},'replacement-metatable')
  local t=mark(setmetatable({},old),'module')
  setmetatable(old,{__newindex=function()debug.setmetatable(t,replacement);collectgarbage()end})
  return package.seeall,pack(t,tag)
end)
local filename='package-stacks-module.lua'
local file=assert(io.open(filename,'wb'));file:write('return 17');assert(file:close())
for _,spec in ipairs{
  {'lua-found',2,filename,'stack.file'},
  {'lua-missing',2,';absent-?.lua;;other-?.lua','stack.file'},
  {'lua-numeric-path',2,12345,'stack.file'},
  {'lua-numeric-name',2,filename,12345},
  {'lua-nul-name',2,filename,'stack.file\0suffix'},
  {'c-missing',3,';absent-?.dll;;other-?.dll','stack.file'},
  {'croot-simple',4,'absent-?.dll','stack'},
  {'croot-missing',4,';absent-?.dll;;other-?.dll','stack.file'},
  {'croot-numeric',4,'absent-?.dll',12345},
}do
  probe(spec[1],function()
    if spec[2]==2 then package.path=spec[3]else package.cpath=spec[3]end
    return original_loaders[spec[2]],pack(spec[4],tag),function()
      package.path=original_path;package.cpath=original_cpath
    end
  end)
end
probe('require-lua-file',function()
  package.path=filename;package.loaded.stack_file=nil
  return require,pack('stack_file',tag),function()package.path=original_path;package.loaded.stack_file=nil end
end)
assert(os.remove(filename))
-- Inspect the same windows while library operations are awaiting callbacks.
local function implementation(fn)if BODY_COPY then return boundary_body_copy(fn)end;return fn end
local function snapshot(event,rows)
  local row={event}
  for i=1,24 do local name,value=debug.getlocal(3,i);if not name then break end
    row[#row+1]=encode(value)
  end
  rows[#rows+1]=table.concat(row,',')
end
local original_loaded=debug.getregistry()._LOADED
do
  local rows={};local backing={};local loaded=mark({},'live-loaded')
  setmetatable(loaded,{
    __index=function(t,k)snapshot('get',rows);collectgarbage();return backing[k]end,
    __newindex=function(t,k,v)snapshot('set:'..type(v),rows);collectgarbage();backing[k]=v end
  })
  debug.getregistry()._LOADED=loaded
  package.loaders=mark({function()
    snapshot('loader',rows);assert(debug.setlocal(2,3,tag))
    return function()snapshot('module',rows)end
  end},'live-loaders')
  local result=implementation(require)('live_import',tag)
  print('LIVE','require',encode(result),table.concat(rows,'|'))
  debug.getregistry()._LOADED=original_loaded;package.loaders=original_loaders
end
do
  local rows={};local target=mark({},'seeall-module');local mt=mark({},'seeall-metatable')
  debug.setmetatable(target,mt)
  setmetatable(mt,{__newindex=function()snapshot('seeall',rows);collectgarbage()end})
  implementation(package.seeall)(target,tag)
  print('LIVE','seeall',table.concat(rows,'|'))
end
do
  local rows={};local lookup=implementation(original_loaders[1])
  package.preload=mark(setmetatable({},{__index=function()snapshot('preload',rows);collectgarbage();return 17 end}),'live-preload')
  local result=lookup('live_lookup',tag)
  print('LIVE','preload',encode(result),table.concat(rows,'|'))
  package.preload=original_preload
end
do
  local rows={};local m=mark(setmetatable({},{
    __index=function()snapshot('get-name',rows);collectgarbage()end,
    __newindex=function(t,k,v)snapshot('set-'..k,rows);collectgarbage();rawset(t,k,v)end
  }),'live-module')
  package.loaded.live_module=m
  local mod=implementation(module)
  local function f()mod('live_module',function()snapshot('option',rows)end)end
  f();print('LIVE','module',table.concat(rows,'|'));package.loaded.live_module=nil
end
do
  local rows={};package.path=nil;package.cpath=nil
  -- findfile keeps raw pointers into these translated strings. Retain them
  -- while replacing their C stack slots, including across the full collection.
  local retained={}
  local old_mt=debug.getmetatable(package)
  setmetatable(package,{__index=function(t,k)
    snapshot(k,rows)
    local _,value=debug.getlocal(2,3);retained[#retained+1]=value
    if k=='cpath'then local _,value=debug.getlocal(2,4);retained[#retained+1]=value end
    assert(debug.setlocal(2,1,'changed-argument'))
    assert(debug.setlocal(2,3,'changed-name'))
    if k=='cpath'then assert(debug.setlocal(2,4,'changed-translation'))end
    collectgarbage();return 'absent-?.missing'
  end})
  local lua_search=implementation(original_loaders[2]);local root_search=implementation(original_loaders[4])
  print('LIVE','lua-path',encode(lua_search('original.name',tag)))
  print('LIVE','root-path',encode(root_search('original.name',tag)))
  print('LIVE','paths',table.concat(rows,'|'));assert(#retained==3)
  setmetatable(package,old_mt);package.path=original_path;package.cpath=original_cpath
end
do
  local finalized=false;local loaded=setmetatable({}, {
    __index=(function()local n=0;return function()
      n=n+1;if n==1 then return nil end;collectgarbage();collectgarbage();return 17
    end end)(),__newindex=function()end
  })
  debug.getregistry()._LOADED=loaded
  package.preload.body_lifetime=function()
    local p=newproxy(true);getmetatable(p).__gc=function()finalized=true end;return p
  end
  local value=implementation(require)('body_lifetime')
  print('LIVE','consumed-result',value,finalized)
  debug.getregistry()._LOADED=original_loaded;package.preload.body_lifetime=nil
end
do
  local finalized=false;local observed=false
  local m=setmetatable({},{__index=function(t,k)
    if k=='_NAME'then local p=newproxy(true);getmetatable(p).__gc=function()finalized=true end;return p end
  end})
  package.loaded.body_name=m
  local mod=implementation(module)
  local function f()mod('body_name',function()collectgarbage();collectgarbage();observed=finalized end)end
  f();print('LIVE','consumed-name',observed,finalized);package.loaded.body_name=nil
end
do
  local finalized=false;local observed=false;local first
  do
    local p=newproxy(true);getmetatable(p).__gc=function()finalized=true end
    first=function()assert(p);assert(debug.setlocal(2,2,nil));first=nil end
  end
  local mod=implementation(module)
  local function second()collectgarbage();collectgarbage();observed=finalized end
  local function f()mod('body_option',first,second)end
  f();print('LIVE','consumed-option',observed,finalized)
  package.loaded.body_option=nil;_G.body_option=nil
end
print('package-stacks-complete',count)
