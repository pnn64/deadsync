-- This fixture starts before any standard library has been opened.
local bare=_G==nil and _VERSION==nil and assert==nil and math==nil and string==nil
  and table==nil and os==nil and debug==nil and coroutine==nil and io==nil and package==nil
local registry,registry_count=library_registry()
-- A direct indexing check also verifies that strings do not have a metatable yet.
local bare_string=library_metatable('')==nil
local names={'base','math','string','table','os','debug'}
local opened={}
local order=library_order
for i=1,6 do
  local index
  if order<=6 then index=(i+order-2)%6+1 else index=(order-i+4)%6+1 end
  local name=names[index]
  local n,s,a,b=library_open(name,'argument',nil,17)
  opened[name]={n=n,s=s,a=a,b=b}
end
assert(bare and bare_string and registry_count==0)
assert(registry==debug.getregistry() and io==nil and package==nil)
assert(_G==getfenv(0) and _VERSION=='Lua 5.1')
local rows={}
local function emit(label,...)
  local row={label}
  for i=1,select('#',...)do row[#row+1]=tostring(select(i,...))end
  rows[#rows+1]=table.concat(row,'\t')
end
emit('bare',bare,bare_string,io==nil,package==nil)
emit('bare.registry',registry_count)
for _,name in ipairs(names)do
  local v=opened[name];local n=name=='base'and 2 or 1
  assert(v.n==n and v.s.n==3+n and v.s[1]=='argument'and v.s[2]==nil and v.s[3]==17)
  assert(v.s[4]==(name=='base'and _G or _G[name])and v.a==v.s[4])
  if name=='base'then assert(v.b==coroutine and v.s[5]==coroutine)end
  emit('open.'..name,v.n,v.s.n,v.a==v.s[4],registry._LOADED[name=='base'and '_G'or name]==v.a)
end

local members={
  math={'abs','acos','asin','atan2','atan','ceil','cosh','cos','deg','exp','floor','fmod','frexp','ldexp','log10','log','max','min','modf','pow','rad','random','randomseed','sinh','sin','sqrt','tanh','tan','mod'},
  string={'byte','char','dump','find','format','gfind','gmatch','gsub','len','lower','match','rep','reverse','sub','upper'},
  table={'concat','foreach','foreachi','getn','maxn','insert','remove','setn','sort'},
  os={'clock','date','difftime','execute','exit','getenv','remove','rename','setlocale','time','tmpname'},
  debug={'debug','getfenv','gethook','getinfo','getlocal','getregistry','getmetatable','getupvalue','setfenv','sethook','setlocal','setmetatable','setupvalue','traceback'},
  coroutine={'create','resume','running','status','wrap','yield'},
}
for _,name in ipairs{'math','string','table','os','debug','coroutine'}do
  local lib=_G[name]
  for _,field in ipairs(members[name])do
    assert(type(lib[field])=='function'and debug.getinfo(lib[field]).what=='C',name..'.'..field)
  end
  emit('members.'..name,#members[name])
end
assert(math.mod==math.fmod and string.gfind==string.gmatch)
emit('aliases',math.mod==math.fmod,string.gfind==string.gmatch,math.pi==3.141592653589793,math.huge>1e308)

local t={3,1,2};table.sort(t);table.insert(t,2,4);assert(table.remove(t,2)==4)
assert(table.concat(t,',')=='1,2,3'and table.maxn(t)==3 and table.getn(t)==3)
assert(math.abs(-17)==17 and math.min(8,3)==3 and math.max(8,3)==8 and math.pow(2,5)==32)
assert(math.floor(1.7)==1 and math.ceil(1.2)==2 and math.sin(0)==0 and math.cos(0)==1)
assert(math.ldexp(.5,4)==8 and math.frexp(8)==.5 and math.fmod(7,3)==1)
assert(string.sub('abc',2)=='bc'and ('abc'):sub(2)=='bc'and string.rep('ab',2)=='abab')
assert(string.char(string.byte('A'))=='A'and string.reverse('ab')=='ba')
assert(string.lower('AZ')=='az'and string.upper('az')=='AZ')
assert(string.find('abc','b')==2 and string.match('a17','%d+')=='17')
local iter=string.gmatch('a1b2','%d');assert(iter()=='1'and iter()=='2'and iter()==nil)
assert(string.gsub('a1b2','%d','x')=='axbx'and string.format('%02d',7)=='07')
assert(loadstring(string.dump(function(x)return x+1 end))(16)==17)
assert(os.difftime(9,4)==5 and type(os.clock())=='number')
assert(type(os.date('!%Y',0))=='string'and type(os.time{year=2020,month=1,day=2})=='number')
assert(type(os.setlocale(nil))=='string')
emit('bodies',table.concat(t,','),string.format('%02d',7),os.difftime(9,4))
assert(pcall(function()return 17 end)==true)
local ok,value=xpcall(function()error('probe',0)end,function(e)return e..'!'end)
assert(not ok and value=='probe!')
assert(tonumber('17')==17 and select(2,1,2)==2 and select('#',1,nil,2)==3)
assert(rawget(rawset({},'a',17),'a')==17 and rawequal(_G,_G))
local proxy=newproxy(true);assert(type(newproxy(proxy))=='userdata')
local pair_iter=select(2,debug.getupvalue(pairs,1));assert(pair_iter~=next)
emit('base.bodies',value,pair_iter~=next,coroutine.running()==nil)

-- Direct calls expose the complete C window, including extra arguments.
local function raw(label,fn,...)
  local n,s,a,b,c=library_rawcall(fn,...)
  emit('raw.'..label,n,s.n,type(a),type(b),type(c))
  return n,s,a,b,c
end
local fn=function(x)coroutine.yield(x);return 29 end
local n,s,co=raw('create',coroutine.create,fn,'extra',nil)
assert(n==1 and s.n==4 and s[1]==fn and s[2]=='extra'and s[3]==nil and s[4]==co)
assert(type(co)=='thread')
local n,s,status=raw('status',coroutine.status,co,'extra',nil)
assert(n==1 and s.n==4 and s[1]==co and s[4]=='suspended'and status=='suspended')
local n,s,value=raw('running.main',coroutine.running,'extra',nil)
assert(n==1 and s.n==4 and s[1]=='extra'and s[2]==nil and type(s[3])=='thread'and value==nil)
local n,s,wrap=raw('wrap',coroutine.wrap,fn,'extra',nil)
assert(n==1 and s.n==4 and s[1]==fn and type(wrap)=='function')
assert(wrap(17)==17 and wrap()==29)
local n,s,ok,value=raw('resume.first',coroutine.resume,co,17)
assert(n==2 and s.n==3 and s[1]==co and s[2]==true and s[3]==17 and ok and value==17)
local n,s,ok,value=raw('resume.last',coroutine.resume,co)
assert(n==2 and s.n==3 and ok and value==29)
local n,s,ok,value=raw('resume.dead',coroutine.resume,co,'extra',nil)
assert(n==2 and s.n==5 and s[2]=='extra'and s[3]==nil and not ok and value=='cannot resume dead coroutine')
local child=coroutine.create(function()
  local n,s,current=library_rawcall(coroutine.running,'extra',nil)
  assert(n==1 and s.n==3 and s[1]=='extra'and s[2]==nil and s[3]==current)
  assert(current==coroutine.running());return 'running'
end)
local ok,value=coroutine.resume(child);assert(ok and value=='running');emit('raw.running.child',ok,value)

-- Reopening reuses the registered table and replaces each C closure.
for _,name in ipairs{'math','string','table','os','debug'}do
  local lib=_G[name];local field=members[name][1];local old=lib[field]
  local oldmeta=debug.getmetatable('')
  local n,s,new=library_open(name)
  assert(n==1 and s.n==1 and new==lib and new[field]~=old)
  emit('reopen.'..name,new==lib,new[field]~=old,debug.getfenv(new[field])==_G)
  if name=='string'then
    local mt=debug.getmetatable('');assert(mt~=oldmeta and mt.__index==lib)
    emit('string.metatable',mt~=oldmeta,mt.__index==lib)
  end
end
local oldassert,oldpairs,oldproxy=assert,pairs,newproxy(true)
local n,s,g,c=library_open('base',17,nil)
assert(n==2 and s.n==4 and g==_G and c==coroutine and assert~=oldassert and pairs~=oldpairs)
assert(pcall(newproxy,oldproxy)==false)
emit('reopen.base',g==_G,c==coroutine,assert~=oldassert,pairs~=oldpairs,pcall(newproxy,oldproxy))

-- Loaded tables take precedence over globals without rewriting those globals.
local loaded=registry._LOADED
for _,name in ipairs{'math','string','table','os','debug'}do
  local global=_G[name];local cached={marker=name};loaded[name]=cached
  local n,s,value=library_open(name)
  assert(value==cached and _G[name]==global and cached.marker==name)
  emit('loaded.'..name,value==cached,_G[name]==global,type(value[members[name][1]]))
  loaded[name]=global
end
library_open('string') -- Restore the default string-method table.

-- Registration order and intermediate values are observable through setters.
for _,name in ipairs{'math','string','table','os','debug'}do
  local events,captured={},{}
  local custom=setmetatable({},{__newindex=function(t,k,v)
    events[#events+1]=k;captured[k]=v;rawset(t,k,v)
  end})
  local prior=loaded[name];loaded[name]=custom
  local _,_,value=library_open(name)
  assert(value==custom)
  local expected={}
  for _,field in ipairs(members[name])do expected[#expected+1]=field end
  if name=='math'then expected[#expected+1]='pi';expected[#expected+1]='huge' end
  -- The compatibility alias is installed after pi and huge.
  if name=='math'then table.remove(expected,#members[name]);expected[#expected+1]='mod'end
  assert(table.concat(events,',')==table.concat(expected,','))
  emit('setters.'..name,table.concat(events,','))
  if name=='string'then
    local ok,err=pcall(captured.gfind)
    assert(not ok and err:match("'string.gfind' was renamed to 'string.gmatch'$"))
    assert(captured.gfind~=custom.gfind and custom.gfind==custom.gmatch)
    emit('string.gfind.intermediate',ok,err:match("'string.gfind'.*$"),captured.gfind~=custom.gfind)
  end
  loaded[name]=prior
end
library_open('string')

-- A registry lookup may return a table through __index without caching it.
do
  local cached={};local proxy=setmetatable({},{__index=function(t,key)
    if key=='math'then return cached end
  end})
  registry._LOADED=proxy
  local _,_,value=library_open('math')
  assert(value==cached and rawget(proxy,'math')==nil and math~=cached)
  emit('loaded.index',value==cached,rawget(proxy,'math')==nil,math~=cached)
  registry._LOADED=loaded
end

-- Failure leaves the successfully registered prefix visible.
do
  local marker={};local count=0;local partial=setmetatable({},{__newindex=function(t,k,v)
    count=count+1;if count==4 then error(marker,0)end;rawset(t,k,v)
  end})
  local prior=loaded.math;loaded.math=partial
  local ok,err=pcall(library_open,'math')
  assert(not ok and err==marker and count==4 and partial.abs and partial.acos and partial.asin and partial.atan2==nil)
  emit('partial.error',ok,err==marker,count,type(partial.abs),partial.atan2==nil)
  loaded.math=prior
end
do
  local prior=math;math=17;loaded.math=nil
  local ok,err=pcall(library_open,'math')
  assert(not ok and err:match("name conflict for module 'math'$"))
  emit('global.conflict',ok,err:match("name conflict.*$"))
  math=prior;loaded.math=prior
end

local oldenv=debug.getfenv(library_open);local env={marker='initializer'}
debug.setfenv(library_open,env)
local _,_,m=library_open('math')
assert(debug.getfenv(m.sin)==env and debug.getfenv(m.mod)==env)
emit('environment',debug.getfenv(m.sin)==env,debug.getfenv(m.mod)==env)
debug.setfenv(library_open,oldenv);library_open('math')

-- Selected states retain their entry/resumption stack and use their globals.
for _,phase in ipairs{'fresh','yielded','dead'}do
  for _,name in ipairs(names)do
    local co=coroutine.create(function()coroutine.yield('yielded');return 'finished'end)
    if phase~='fresh'then assert(coroutine.resume(co))end
    if phase=='dead'then assert(coroutine.resume(co))end
    local env={marker=phase};debug.setfenv(co,env)
    local key=name=='base'and '_G'or name
    local prior=loaded[key];local priorco=loaded.coroutine
    loaded[key]=nil;if name=='base'then loaded.coroutine=nil end
    local n,s,value,ct=library_open_thread(co,name,'argument',nil,17)
    assert(s.n==3+n and s[1]=='argument'and s[2]==nil and s[3]==17 and s[4]==value)
    assert(value==env[key] and loaded[key]==value)
    if name=='base'then assert(ct==env.coroutine and env._G==env and env._VERSION=='Lua 5.1')end
    local field=name=='base'and 'assert'or members[name][1]
    local expected=phase=='yielded'and _G or env
    assert(debug.getfenv(value[field])==expected)
    emit('thread.'..phase..'.'..name,n,s.n,value==env[key],coroutine.status(co),debug.getfenv(value[field])==expected)
    loaded[key]=prior;loaded.coroutine=priorco
    if phase=='fresh'then assert(coroutine.resume(co))end
    if phase~='dead'then local ok,value=coroutine.resume(co);assert(ok and value=='finished')end
  end
end
library_open('string')
emit('complete',#rows+1)
return table.concat(rows,'\n')..'\n'
