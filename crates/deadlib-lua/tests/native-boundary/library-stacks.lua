do local co=coroutine.create(function()boundary_state()end);assert(coroutine.resume(co))end
collectgarbage()
local function pack(...)return {n=select('#',...),...}end
local labels=setmetatable({},{__mode='k'})
local function mark(value,label)labels[value]=label;return value end
mark(_G,'globals');mark(debug.getregistry(),'registry')
local function encode(value)
  local t=type(value)
  if t=='nil'or t=='boolean'or t=='number'then return tostring(value)end
  if t=='string'then return(value:gsub('.',function(c)return('%02x'):format(c:byte())end))end
  if labels[value]then return t..':'..labels[value]end
  if t=='function'then local d=debug.getinfo(value,'Su');return 'function:'..d.what..':'..d.nups end
  if t=='userdata'then return 'userdata:'..tostring(io.type(value))end
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
local function fixed(name,fn,...)
  local args=pack(...);probe(name,function()return fn,args end)
end
local tag=mark({},'tag');local mt=mark({},'metatable')
local function fresh_table()return mark({17,29,42},'table')end
for _,spec in ipairs{
 {'getn',function(t)return pack(t,tag)end},
 {'maxn',function(t)return pack(t,tag)end},
 {'concat',function(t)return pack(t,17,1,3,tag)end},
 {'concat-empty',function(t)return pack(t,':',2,1,tag)end},
 {'insert',function(t)return pack(t,17)end},
 {'insert-position',function(t)return pack(t,2,17)end},
 {'remove',function(t)return pack(t,2,tag)end},
 {'remove-empty',function(t)return pack(t,99,tag)end},
 {'sort',function(t)return pack(t,nil,tag)end},
 {'sort-function',function(t)return pack(t,function(a,b)return a>b end,tag)end},
 {'foreach',function(t)return pack(t,function()end,tag)end},
 {'foreach-stop',function(t)return pack(t,function()return tag end,tag)end},
 {'foreachi',function(t)return pack(t,function()end,tag)end},
 {'foreachi-stop',function(t)return pack(t,function()return tag end,tag)end},
}do
  local name,make=spec[1],spec[2];local fn=table[name:match('^[^-]+')]
  probe('table-'..name,function()return fn,make(fresh_table())end)
end
local f=(function(x)return function()return x end end)(17)
local function suspended()
  local co=coroutine.create(function(x)coroutine.yield();return x end)
  assert(coroutine.resume(co,17));return co
end
fixed('debug-registry',debug.getregistry,17,tag)
fixed('debug-getmetatable',debug.getmetatable,setmetatable({},mt),tag)
fixed('debug-getmetatable-nil',debug.getmetatable,{},tag)
probe('debug-setmetatable',function()return debug.setmetatable,pack({},mt,tag)end)
fixed('debug-getfenv',debug.getfenv,f,tag)
probe('debug-setfenv',function()return debug.setfenv,pack(function()end,{},tag)end)
fixed('debug-getinfo',debug.getinfo,f,'SufL',tag)
fixed('debug-getinfo-level',debug.getinfo,0,'Su',tag)
fixed('debug-getinfo-missing',debug.getinfo,999,'Su',tag)
fixed('debug-getlocal',debug.getlocal,0,1,tag)
fixed('debug-getlocal-missing',debug.getlocal,0,99,tag)
fixed('debug-setlocal',debug.setlocal,0,1,tag,29)
fixed('debug-setlocal-missing',debug.setlocal,0,99,tag,29)
fixed('debug-setlocal-thread',debug.setlocal,0,1,coroutine.create(function()end),29)
probe('debug-getlocal-other',function()return debug.getlocal,pack(suspended(),1,1,tag)end)
probe('debug-setlocal-other',function()return debug.setlocal,pack(suspended(),1,1,tag,29)end)
probe('debug-getinfo-other',function()return debug.getinfo,pack(suspended(),1,'fL',tag)end)
probe('debug-gethook-other',function()return debug.gethook,pack(suspended(),tag)end)
probe('debug-sethook-clear-other',function()return debug.sethook,pack(suspended(),nil,'c',17,tag)end)
probe('debug-sethook-other',function()return debug.sethook,pack(suspended(),function()end,'',0,tag)end)
fixed('debug-getupvalue',debug.getupvalue,f,1,tag)
fixed('debug-getupvalue-missing',debug.getupvalue,f,99,tag)
fixed('debug-getupvalue-native',debug.getupvalue,print,1,tag)
fixed('debug-setupvalue',debug.setupvalue,f,1,29,tag)
fixed('debug-setupvalue-missing',debug.setupvalue,f,99,29,tag)
fixed('debug-setupvalue-native',debug.setupvalue,print,1,29,tag)
print('setupvalue-top',f()==tag)
fixed('debug-gethook',debug.gethook,nil,tag)
fixed('debug-traceback-value',debug.traceback,17,tag)
fixed('debug-traceback-level',debug.traceback,'message',999,tag)
probe('debug-traceback-other',function()return debug.traceback,pack(suspended(),'message',999,tag)end)
fixed('os-difftime',os.difftime,12345,12300,tag)
fixed('os-getenv',os.getenv,'DEADLUA_ABSENT_LIBRARY_STACK_VARIABLE',tag)
fixed('os-date',os.date,'!%Y-%m-%d',1234567890,tag)
fixed('os-date-table',os.date,'!*t',1234567890,tag)
probe('os-time',function()return os.time,pack({year=2009,month=2,day=13,hour=12,min=0,sec=0,isdst=false},tag)end)
fixed('os-setlocale',os.setlocale,'C','all',tag)
fixed('os-remove-failed',os.remove,'absent-library-stacks-file',tag)
fixed('os-rename-failed',os.rename,'absent-library-stacks-file','absent-library-stacks-destination',tag)
local filename='library-stacks-input.txt'
local file=assert(io.open(filename,'wb'));file:write('17\nhello\n');file:close()
local function fresh_file()
  local file=mark(assert(io.tmpfile()),'file');file:write('17\nhello\n');file:seek('set',0)
  return file
end
local methods=getmetatable(io.stdout).__index
for _,spec in ipairs{
 {'read-number',methods.read,function(f)return pack(f,'*n',tag)end},
 {'read-line',methods.read,function(f)return pack(f,'*l',tag)end},
 {'read-chars',methods.read,function(f)return pack(f,2,tag)end},
  {'read-default',methods.read,function(f)return pack(f)end},
  {'read-number-valid',methods.read,function(f)return pack(f,'*n','*l')end},
  {'read-line-valid',methods.read,function(f)return pack(f,'*l')end},
  {'read-chars-valid',methods.read,function(f)return pack(f,2)end},
  {'read-all',methods.read,function(f)return pack(f,'*a')end},
  {'read-end',methods.read,function(f)f:seek('end',0);return pack(f,'*l')end},
 {'write',methods.write,function(f)return pack(f,17,'abc')end},
 {'flush',methods.flush,function(f)return pack(f,tag)end},
 {'seek',methods.seek,function(f)return pack(f,'set',2,tag)end},
 {'setvbuf',methods.setvbuf,function(f)return pack(f,'full',128,tag)end},
 {'lines',methods.lines,function(f)return pack(f,tag)end},
 {'close',methods.close,function(f)return pack(f,tag)end},
  {'gc',getmetatable(io.stdout).__gc,function(f)return pack(f,tag)end},
  {'gc-closed',getmetatable(io.stdout).__gc,function(f)f:close();return pack(f,tag)end},
}do
  probe('file-'..spec[1],function()
    local f=fresh_file()
    return spec[2],spec[3](f),function()if io.type(f)=='file'then f:close()end end
  end)
end
fixed('io-type',io.type,tag,17)
fixed('io-type-file',io.type,io.stdout,tag)
fixed('io-type-proxy',io.type,newproxy(true),tag)
local null=mark(boundary_pointer(nil),'null');local pointer=mark(boundary_pointer(tag),'pointer')
debug.setmetatable(null,{})
fixed('io-type-null',io.type,null,tag)
fixed('io-type-pointer',io.type,pointer,tag)
debug.setmetatable(null,nil)
fixed('io-close-stdout',io.close,io.stdout,tag)
fixed('io-open',io.open,filename,'r',tag)
fixed('io-open-failed',io.open,'absent-library-stacks-file','r',tag)
fixed('io-tmpfile',io.tmpfile,17,tag)
fixed('io-popen',io.popen,'exit 0','r',tag)
fixed('io-input-default',io.input,nil,tag)
fixed('io-output-default',io.output,nil,tag)
local original_input=io.input();local original_output=io.output()
for _,spec in ipairs{
 {'input',io.input}, {'output',io.output},
}do
 probe('io-'..spec[1]..'-file',function()
  local f=fresh_file()
  return spec[2],pack(f,tag),function()io.input(original_input);io.output(original_output);f:close()end
 end)
end
for _,spec in ipairs{
 {'read',io.read,function(f)io.input(f);return pack('*n','*l')end},
 {'write',io.write,function(f)io.output(f);return pack(17,'abc')end},
 {'flush',io.flush,function(f)io.output(f);return pack(tag)end},
 {'lines',io.lines,function(f)io.input(f);return pack(nil,tag)end},
  {'lines-default',io.lines,function(f)io.input(f);return pack()end},
  {'close-default',io.close,function(f)io.output(f);return pack()end},
  {'lines-closed-default',io.lines,function(f)io.input(f);f:close();return pack()end},
  {'close-closed-default',io.close,function(f)io.output(f);f:close();return pack()end},
}do
 probe('io-'..spec[1],function()
  local f=fresh_file()
  return spec[2],spec[3](f),function()io.input(original_input);io.output(original_output);if io.type(f)=='file'then f:close()end end
 end)
end
fixed('io-lines-file',io.lines,filename,tag)
probe('file-lines-iterator',function()
 local f=fresh_file();local next=f:lines()
 return next,pack(tag),function()f:close()end
end)
probe('file-tostring-closed',function()
 local f=fresh_file();f:close()
 return getmetatable(f).__tostring,pack(f,tag)
end)
probe('file-lines-end',function()
 local f=fresh_file();f:seek('end',0);local next=f:lines()
 return next,pack(tag),function()f:close()end
end)
probe('io-lines-iterator-end',function()
 local next=io.lines(filename);while next()do end
 -- A closed iterator must raise without delivering a return hook.
 return next,pack(tag)
end)
probe('io-lines-iterator-close',function()
 local next=io.lines(filename);assert(next());assert(next())
 return next,pack(tag)
end)
assert(os.remove(filename))
print('library-stacks-complete',count)
