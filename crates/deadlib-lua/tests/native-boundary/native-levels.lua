local create,copy,level,resume,push,values=boundary_newthread,boundary_setlevel,boundary_level,boundary_resume,boundary_thread_push,boundary_thread_values
local function here()local co=boundary_thread();return level(co)end
local main=boundary_thread();local saved=create();copy(main,saved)
print('level-main',level(main),level(saved))
do
 local zero=create();local co=create(function()return here()end)
 print('level-fresh',level(co),level(zero));copy(main,co);print('level-copy',level(co))
 print('level-resume',resume(co,0),level(co));local n,t=values(co);print('level-result',n,t[1])
 co=create(function()return here()end);print('level-lua-resume',coroutine.resume(co),level(co))
 co=create(function()return boundary_call(function()return boundary_call(here,0)end,0)end)
 print('level-reentry',resume(co,0),level(co));n,t=values(co);print('level-reentry-result',n,t[1])
end
do
 local co;co=create(function()return boundary_call(function()return boundary_call(function()error('bad',0)end,0)end,0)end)
 print('level-fault',resume(co,0),level(co));local n,t=values(co);print('level-fault-window',n,t[n])
 local other=create();copy(co,other);print('level-fault-copy',level(other))
end
do
 local co;local zero=create();co=create(function()
  print('level-active-before',here());copy(zero,co);print('level-active-zero',here());return here()
 end)
 print('level-active-end',resume(co,0),level(co));print('level-active-reject',resume(co,0),level(co))
end
do
 local co;local zero=create();co=create(function()
  local save=create();copy(co,save)
  local ok,e=pcall(function()copy(zero,co);error('bad',0)end)
  print('level-pcall-error',ok,e,here())
  ok,e=pcall(function()copy(zero,co);return 42 end)
  print('level-pcall-ok',ok,e,here());copy(save,co);return here()
 end)
 print('level-pcall-end',resume(co,0),level(co))
end
do
 local co=create(function()
  local zero=create();print('level-normal-before',level(main));copy(zero,main);print('level-normal-zero',level(main));return 42
 end)
 print('level-normal-end',resume(co,0),level(main));copy(saved,main);print('level-normal-restored',level(main))
end
for reset=0,6 do
 local co=create(function()
  local zero=create();local seen={};local chunks={'return ','(','(','17',')',')'}
  local status,f,n=boundary_level_reader(zero,function(i)seen[#seen+1]=here();return chunks[i]end,reset)
  print('level-reader',reset,status,type(f),n,table.concat(seen,','),here())
  return 42
 end)
 print('level-reader-end',reset,resume(co,0),level(co))
end
for reset=0,3 do
 local co=create(function()
  local zero=create();local seen={};local chunks={'local',' x =',' )'}
  local status,f,n=boundary_level_reader(zero,function(i)seen[#seen+1]=here();return chunks[i]end,reset)
  print('level-reader-error',reset,status,f,n,table.concat(seen,','),here());return 42
 end)
 print('level-reader-error-end',reset,resume(co,0),level(co))
end
do
 local co=create(function()
  local current=boundary_thread();local rows={};local busy=false
  debug.sethook(function(event)
   if not busy then
    busy=true
    local info=debug.getinfo(2,'nS');rows[#rows+1]=event..':'..info.what..':'..here()
    busy=false
   end
  end,'cr')
  local ok,v=pcall(function()return tostring(setmetatable({},{__tostring=function()return 'ok'end}))end)
  debug.sethook(nil);print('level-hooks',ok,v,table.concat(rows,','));return 42
 end)
 print('level-hooks-end',resume(co,0),level(co))
end
do
 local co=create(function()
  collectgarbage();local seen={}
  do local p=newproxy(true);getmetatable(p).__gc=function()seen[#seen+1]=here()end end
  boundary_gc(2);print('level-native-gc',table.concat(seen,','),here())
  do local p=newproxy(true);getmetatable(p).__gc=function()seen[#seen+1]=here()end end
  collectgarbage();print('level-lua-gc',table.concat(seen,','),here());return 42
 end)
 print('level-gc-end',resume(co,0),level(co))
end
do
 local co;local zero=create();co=create(function()
  pcall(function()copy(zero,co)end)
  local v=boundary_call(here,0);print('level-wrapped-call',v,here());return 42
 end)
 print('level-wrapped-end',resume(co,0),level(co))
end
do
 local high=create(function()return here()end)
 local exact=create(function()return here()end)
 local function nest(n,callback)if n==0 then return callback()end;return boundary_call(function()return nest(n-1,callback)end,0)end
 local co;co=create(function()
  local current=boundary_thread()
  nest(198,function()copy(current,exact)end);print('level-capture-exact',level(exact),here())
  local ok,e=xpcall(function()nest(500,function()end)end,function(e)copy(current,high);print('level-capture-high',here(),level(high));return e end)
  print('level-overflow-recovery',ok,type(e),here());return 42
 end)
 print('level-overflow-end',resume(co,0),level(co))
 print('level-exact-resume',resume(exact,0),level(exact));local n,t=values(exact);print('level-exact-result',n,t[1])
 print('level-high-reject',resume(high,0),boundary_thread_status(high),level(high));n,t=values(high);print('level-high-error',n,t[1])
 boundary_thread_resize(high,0);push(high,function()return here()end);copy(main,high)
 print('level-high-recovered',resume(high,0),level(high))
end
for _,size in ipairs{1,7,31,4096}do
 for reset=0,2 do
  local co=create(function()
   local input=assert(io.open('reader-roots-nested.luac','rb'));local bytes=input:read('*a');input:close()
   local zero=create();local seen={}
   local status,f,n=boundary_level_reader(zero,function(i)
    seen[#seen+1]=here();local chunk=bytes:sub((i-1)*size+1,i*size);if chunk~=''then return chunk end
   end,reset)
   print('level-reader-binary',size,reset,status,type(f),n,table.concat(seen,','),here());return 42
  end)
  print('level-reader-binary-end',size,reset,resume(co,0),level(co))
 end
end
do
 local co=create(function()
  local zero=create();local chunks={'return ','42'};local seen={};local parser_thread=boundary_thread()
  local status,f,n=boundary_level_reader(zero,function(i)
   if i==2 then
    local inner=create(function()copy(zero,parser_thread);return 42 end)
    print('level-reader-normal-child',resume(inner,0),level(parser_thread))
   end
   seen[#seen+1]=here();return chunks[i]
  end,0)
  print('level-reader-normal',status,type(f),n,table.concat(seen,','),here());return 42
 end)
 print('level-reader-normal-end',resume(co,0),level(co));copy(saved,main)
end
for _,yield in ipairs{boundary_yield,coroutine.yield}do
 local co=create(function()copy(main,boundary_thread());return yield(17)end)
 local result,status=resume(co,0);assert(result==2 and status==2 and level(co)==1)
 local n,t=values(co);assert(t[n]=='attempt to yield across metamethod/C-call boundary')
 print('level-raised-yield',result,status,level(co),n,t[n])
end
do
 local co=create(function()
  local zero=create();local chunks={'return ','42'};local seen={}
  local status,f,n=boundary_level_reader(zero,function(i)
   if i==2 then
    local before=here()
    local st,e,count=boundary_level_reader(zero,function(j)return ({'local x =',')'})[j]end,1)
    assert(st==3 and type(e)=='string' and here()==before)
    print('level-nested-reader',before,here(),st,count)
   end
   seen[#seen+1]=here();return chunks[i]
  end,0)
  assert(status==0 and f()==42 and here()==1)
  print('level-nested-reader-outer',status,n,table.concat(seen,','),here());return 42
 end)
 assert(resume(co,0)==0 and level(co)==0)
end
do
 local zero=create();local checked=0
 for i=1,1000 do
  local co=create(function(a)local b=boundary_yield(a);return b,here()end)
  copy(main,co);push(co,i);assert(resume(co,1)==1 and level(co)==level(main))
  copy(zero,co)
  if i%2==0 then
   push(co,i+1);assert(resume(co,1)==0);local n,t=values(co)
   assert(n==2 and t[1]==i+1 and t[2]==1 and level(co)==0)
  else
   local ok,v,count=coroutine.resume(co,i+1);assert(ok and v==i+1 and count==level(main)+1 and level(co)==level(main))
  end
  checked=checked+1;if i%13==0 then collectgarbage()end
 end
 print('native-call-levels-complete',checked)
end
collectgarbage()
