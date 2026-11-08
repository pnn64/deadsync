-- Open I/O before any other library, through the unmodified public C API.
local bare=_G==nil and io==nil and package==nil and assert==nil
local registry,count=library_registry()
local no_string_meta=library_metatable('')==nil
local n,s,first=library_open('io','argument',nil,17)
local names={'base','table','string','math','os','debug','package'}
for i=1,7 do
  local index=(i+library_order-2)%7+1
  library_open(names[index])
end
assert(bare and count==0 and no_string_meta)
local rows={}
local function emit(label,...)
  local row={label}
  for i=1,select('#',...)do row[#row+1]=tostring(select(i,...))end
  rows[#rows+1]=table.concat(row,'\t')
end
emit('bare',bare,count==0,no_string_meta,first==io)
local meta=registry['FILE*']
assert(n==1 and s.n==5 and s[1]=='argument'and s[2]==nil and s[3]==17)
assert(s[4]==meta and s[5]==io and first==io and meta.__index==meta)
assert(package.loaded.io==io)
emit('io.stack',n,s.n,s[4]==meta,s[5]==io)
local env=debug.getfenv(io.close)
local standard=debug.getfenv(io.stdin)
local pipe=debug.getfenv(io.popen)
assert(env~=standard and env~=pipe and standard~=pipe)
assert(env[1]==io.stdin and env[2]==io.stdout and env.__close~=standard.__close)
assert(type(env.__close)=='function'and type(standard.__close)=='function'and type(pipe.__close)=='function')
assert(debug.getfenv(env.__close)==_G and debug.getfenv(standard.__close)==env)
assert(debug.getfenv(pipe.__close)==env and debug.getfenv(library_open)==package)
emit('io.environments',env[1]==io.stdin,env[2]==io.stdout,debug.getfenv(env.__close)==_G)
for _,name in ipairs{'close','flush','input','lines','open','output','read','tmpfile','type','write'}do
  assert(debug.getfenv(io[name])==env and debug.getinfo(io[name]).what=='C')
  emit('io.member.'..name,debug.getfenv(io[name])==env)
end
for _,name in ipairs{'close','flush','lines','read','seek','setvbuf','write','__gc','__tostring'}do
  assert(debug.getfenv(meta[name])==_G and debug.getinfo(meta[name]).what=='C')
  emit('file.member.'..name,debug.getfenv(meta[name])==_G)
end
for kind,name in ipairs{'stdin','stdout','stderr'}do
  local file=io[name]
  assert(type(file)=='userdata'and io.type(file)=='file'and getmetatable(file)==meta)
  local size,pointer=library_standard(file,kind)
  assert(debug.getfenv(file)==standard and size and pointer)
  local ok,message=file:close();assert(ok==nil and message=='cannot close standard file')
  assert(io.type(file)=='file')
  emit('standard.'..name,io.type(file),message)
end
assert(io.input()==io.stdin and io.output()==io.stdout and io.close~=meta.close)
local file=assert(io.tmpfile());assert(getmetatable(file)==meta and debug.getfenv(file)==env)
assert(file:write('first\nsecond\n')and file:seek('set')==0)
local iter=file:lines();assert(iter()=='first'and iter()=='second'and iter()==nil)
assert(file:seek('set')==0 and file:read('*a')=='first\nsecond\n')
assert(file:close()and io.type(file)=='closed file')
emit('file.bodies',io.type(file))
-- Each reopen replaces closures and standard userdata, reusing registered tables.
local saved_io=io
for round=1,4 do
  local old={stdin=io.stdin,stdout=io.stdout,stderr=io.stderr,read=io.read,close=meta.close}
  local oldenv=debug.getfenv(io.read)
  local caller_env=debug.getfenv(library_open)
  local n,s,new=library_open('io',round,nil,'tail')
  local current=debug.getfenv(io.read)
  assert(n==1 and s.n==5 and new==saved_io and s[4]==meta and s[5]==io)
  assert(current~=oldenv and debug.getfenv(library_open)==current)
  assert(debug.getfenv(meta.close)==caller_env and debug.getfenv(current.__close)==caller_env)
  assert(io.read~=old.read and meta.close~=old.close and io.close~=meta.close)
  assert(io.input()==io.stdin and io.output()==io.stdout and current[1]==io.stdin and current[2]==io.stdout)
  for _,name in ipairs{'stdin','stdout','stderr'}do
    assert(old[name]~=io[name]and io.type(old[name])=='file'and io.type(io[name])=='file')
    assert(debug.getfenv(old[name])~=debug.getfenv(io[name]))
  end
  assert(oldenv[1]==old.stdin and oldenv[2]==old.stdout)
  local f=assert(io.tmpfile());assert(debug.getfenv(f)==current)
  assert(f:write('r',round)and f:seek('set')==0 and f:read('*a')=='r'..round and f:close())
  collectgarbage()
  emit('reopen.'..round,new==saved_io,current~=oldenv,old.stdin~=io.stdin,io.type(old.stdin))
end
-- Existing FILE* and io tables can direct registration through metamethods.
local oldmeta=meta
for _,mode in ipairs{'store','discard','error'}do
  collectgarbage();collectgarbage()
  local calls={};local proxy={}
  setmetatable(proxy,{__newindex=function(t,k,v)
    calls[#calls+1]=k
    if mode=='error'then error('registration-stop',0)end
    if mode=='store'then rawset(t,k,v)end
    collectgarbage()
  end})
  registry['FILE*']=proxy
  local ok,err=pcall(library_open,'io')
  if mode=='error'then assert(not ok and err=='registration-stop'and #calls==1)
  else
    assert(ok and #calls==10 and calls[1]=='__index'and calls[10]=='__tostring',mode..':'..tostring(ok)..':'..#calls..':'..table.concat(calls,',')..':'..tostring(err))
    assert(getmetatable(io.stdin)==proxy and io.type(io.stdin)=='file')
    if mode=='store'then assert(proxy.__index==proxy and type(proxy.close)=='function')end
  end
  emit('metatable.'..mode,ok,#calls,calls[1],calls[#calls])
  for _,name in ipairs{'stdin','stdout','stderr'}do debug.setmetatable(io[name],oldmeta)end
  registry['FILE*']=oldmeta
  library_open('io')
end
-- Registration may allocate files before the initializer attaches its standards.
local early={};local proxy={};local baseenv=debug.getfenv(library_open)
setmetatable(proxy,{__newindex=function(t,k,v)
  rawset(t,k,v)
  if k=='tmpfile'then local f=assert(v());early[1]=f;early[2]=getmetatable(f);early[3]=debug.getfenv(f)end
end})
registry._LOADED.io=proxy;_G.io=proxy
local _,_,new=library_open('io')
assert(new==proxy and early[2]==meta and early[3]==debug.getfenv(io.tmpfile))
assert(io.type(early[1])=='file'and early[1]:close())
emit('early.file',new==proxy,early[2]==meta,early[3]~=baseenv)
-- Selected suspended C activations use their own environments and stack windows.
do
  local child=coroutine.create(function()coroutine.yield('yielded');return 29 end)
  local ok,value=coroutine.resume(child);assert(ok and value=='yielded')
  local n,s,new=library_open_thread(child,'io','thread',nil,17)
  assert(n==1 and s.n==5 and s[1]=='thread'and s[2]==nil and s[3]==17 and s[4]==meta and s[5]==io and new==io)
  local ok,a=coroutine.resume(child);assert(ok and a==29)
  emit('thread.yielded',n,s.n,coroutine.status(child))
  debug.setfenv(coroutine.yield,_G)
end
do
  local parent
  local child=coroutine.create(function()
    local n,s,new=library_open_thread(parent,'io','thread',nil,17)
    assert(n==1 and s.n==5 and s[4]==meta and s[5]==io and new==io)
    emit('thread.waiting',n,s.n,coroutine.status(parent));return 29
  end)
  parent=coroutine.create(function()return coroutine.resume(child)end)
  local ok,child_ok,value=coroutine.resume(parent);assert(ok and child_ok and value==29)
  debug.setfenv(coroutine.resume,_G)
end
emit('complete',#rows)
return table.concat(rows,'\n')..'\n'
