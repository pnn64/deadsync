do local co=coroutine.create(function()boundary_state()end);assert(coroutine.resume(co))end
collectgarbage()
local function pack(...)return {n=select('#',...),...}end
local labels=setmetatable({},{__mode='k'})
local function mark(value,label)labels[value]=label;return value end
mark(_G,'globals');mark(debug.getregistry(),'registry')
local function encode(value,memory)
  local t=type(value)
  if t=='nil' or t=='boolean' then return tostring(value) end
  if t=='number' then return memory and '<memory>' or tostring(value) end
  if t=='string' then return (value:gsub('.',function(c)return ('%02x'):format(c:byte())end)) end
  if labels[value] then return t..':'..labels[value] end
  if t=='function' then
    local d=debug.getinfo(value,'Su');return 'function:'..d.what..':'..d.nups
  end
  if t=='userdata' then return 'userdata:'..tostring(io.type(value)) end
  return t
end
local count=0
local function observe(name,fn,args,memory,index)
  if BODY_COPY then fn=boundary_body_copy(fn)end
  count=count+1
  local log={}
  local co=coroutine.create(function()return fn(unpack(args,1,args.n))end)
  boundary_return_stack(fn,log,co,index,'hook-token')
  local results=pack(coroutine.resume(co))
  print('CASE',name,#log,results.n)
  for i=1,results.n do print('RESULT',i,encode(results[i],memory and i>1)) end
  for _,row in ipairs(log)do
    print('TOP',row.top)
    for i=1,row.top do print('SLOT',i,encode(row[i],memory and i==row.top))end
  end
end
local function probe(name,fn,args,memory)
  for _,index in ipairs{0,1,-1} do observe(name..':'..index,fn,args,memory,index)end
end
local function add(name,fn,... )probe(name,fn,pack(...))end
local tag=mark({},'tag');local tbl=mark({x=17,29},'table')
local mt=mark({},'metatable');local protected=mark({__metatable=tag},'protected-metatable')
local protected_obj=mark(setmetatable({},protected),'protected-object')
local f=mark(function()return 17,'result' end,'lua-function')
local env=mark({answer=42},'environment')
add('assert',assert,17,nil,tag)
add('type',type,17,tag)
add('tonumber-number',tonumber,'17',nil,tag)
add('tonumber-radix',tonumber,17,16,tag)
add('tonumber-nil',tonumber,{},10,tag)
add('rawequal',rawequal,tag,tag,29)
add('rawget',rawget,tbl,'x',tag)
add('rawset',rawset,tbl,'y',29,tag)
add('select-count',select,'#',17,nil,tag)
add('select-values',select,2,17,nil,tag)
add('select-empty',select,99,17,nil,tag)
add('unpack',unpack,{17,nil,29},1,3,tag)
add('unpack-empty',unpack,{},2,1,tag)
add('next',next,{x=17},nil,tag)
add('next-empty',next,{},nil,tag)
add('pairs',pairs,tbl,tag)
add('ipairs',ipairs,tbl,tag)
add('getmetatable-none',getmetatable,tbl,tag)
add('getmetatable',getmetatable,setmetatable({},mt),tag)
add('getmetatable-protected',getmetatable,protected_obj,tag)
add('setmetatable',setmetatable,{},mt,tag)
add('setmetatable-nil',setmetatable,{},nil,tag)
add('getfenv-function',getfenv,f,tag)
add('getfenv-native',getfenv,print,tag)
add('getfenv-zero',getfenv,0,tag)
add('getfenv-default',getfenv)
add('setfenv-function',setfenv,f,env,tag)
add('setfenv-zero',setfenv,0,env,tag)
add('newproxy-none',newproxy)
add('newproxy-false',newproxy,false,tag)
add('newproxy-true',newproxy,true,tag)
add('newproxy-clone',newproxy,newproxy(true),tag)
add('tostring-number',tostring,17,tag)
add('tostring-string',tostring,'abc',tag)
add('tostring-bool',tostring,true,tag)
add('tostring-nil',tostring,nil,tag)
add('tostring-meta',tostring,setmetatable({},{__tostring=function()return 'meta' end}),tag)
add('print',print,'hello',17,'tail')
add('pcall',pcall,f,17,tag)
add('pcall-error',pcall,function()error(tag,0)end)
add('xpcall',xpcall,f,function(e)return e end,tag)
add('xpcall-error',xpcall,function()error(tag,0)end,function(e)return e end,tag)
add('loadstring',loadstring,'return 17','=return-stack',tag)
add('loadstring-number',loadstring,17,'=return-stack',tag)
add('loadstring-error',loadstring,'return +','=return-stack',tag)
local function reader()
  local i=0
  return function()i=i+1;if i==1 then return 'return 17' end end
end
add('load',load,reader(),'=return-reader',tag)
add('load-empty',load,function()end,'=return-empty',tag)
local function pieces(...)
  local parts=pack(...);local i=0
  return function()i=i+1;collectgarbage();return parts[i]end
end
add('load-split',load,pieces('local x=17;','return function()return x end'),'=return-split',tag)
add('load-error',load,pieces('return +'),'=return-error',tag)
add('load-reader-error',load,function()error(tag,0)end,'=reader-error',tag)
add('load-reader-type',load,function()return tag end,'=reader-type',tag)
probe('gcinfo',gcinfo,pack(17,tag),true)
add('gc-collect',collectgarbage,'collect',nil,tag)
add('gc-stop',collectgarbage,'stop',nil,tag)
add('gc-restart',collectgarbage,'restart',nil,tag)
probe('gc-count',collectgarbage,pack('count',nil,tag),true)
for _,entry in ipairs{
  {'len',17,tag},{'sub',12345,2,4,tag},{'rep',17,3,tag},{'reverse',123,tag},
  {'byte',123,1,3,tag},{'byte','abc',2,1,tag},{'char',65,0,255},
  {'lower',123,tag},{'upper','aBc',tag},
  {'find','ab12cd','(%d+)',1,false,tag},{'find','abc','b',1,true,tag},
  {'find','abc','z',1,false,tag},{'match','ab12cd','(%d+)',1,tag},
  {'match','abc','z',1,tag},{'gmatch','a1b2','%d',tag},
  {'gsub','a1b2','%d','X',4,tag},{'gsub','a1b2','%d',function(n)return tonumber(n)+1 end,4,tag},
  {'gsub','a1b2','%d',{['1']='X'},4,tag},
  {'format','%s:%d',17,29,tag},{'format','plain',tag},
} do
  local fn=string[entry[1]]
  local args={n=#entry-1};for i=2,#entry do args[i-1]=entry[i]end
  probe('string-'..entry[1]..'-'..count,fn,args)
end
local gm=string.gmatch('a1b2','%d');add('gmatch-iterator',gm,tag);add('gmatch-iterator2',gm,tag);add('gmatch-done',gm,tag)
add('gfind-alias',string.gfind,'a1b2','%d',tag)
local dumpfile=assert(io.open('dump-stripped.luac','rb'));local binary=dumpfile:read('*a');dumpfile:close()
local dumped=assert(loadstring(binary))
add('string-dump',string.dump,dumped,tag)
add('load-binary',load,pieces(binary),'=return-binary',tag)
add('loadfile',loadfile,'dump-stripped.luac',tag)
add('dofile',dofile,'dump-stripped.luac',tag)
print('return-stacks-complete',count)
