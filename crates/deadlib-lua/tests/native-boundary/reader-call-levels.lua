local create,level,copy,resume,push,values=boundary_newthread,boundary_level,boundary_setlevel,boundary_resume,boundary_thread_push,boundary_thread_values
local main=boundary_thread();local exact=create()
local function nest(n,f)if n==0 then return f()end;return boundary_call(function()return nest(n-1,f)end,0)end
local capturer=create(function()local me=boundary_thread();nest(198,function()copy(me,exact)end);return 42 end)
assert(resume(capturer,0)==0 and level(exact)==199)
local entry=assert(loadstring([[return function(mode,wrapped,native_depth)
 local me=boundary_thread();local zero,keep=boundary_newthread(),boundary_newthread();boundary_setlevel(me,keep)
 if wrapped then pcall(function()boundary_setlevel(zero,me)end)end
 local trace={};local visits=0
 local function reader()
  visits=visits+1;trace[visits]=boundary_level(me)
  if mode=='error' then error('reader error',0)end
  if mode=='invalid' then return {}end
  if mode=='empty' then return nil end
  if mode=='initial-empty' and visits==1 then return ''end
  if visits==(mode=='initial-empty' and 2 or 1)then return mode=='syntax'and 'local x = )'or 'return 42'end
 end
 local f,e=load(reader,'=reader-call-levels')
 boundary_setlevel(keep,me)
 local got=type(f)=='function'and (mode~='empty'and f()or'empty')or'nil'
 return type(f),type(e),visits,table.concat(trace,','),got,type(e)=='string'and e or'',boundary_level(me)
end]],'=reader-level-body'))()
local checked=0
for _,high in ipairs{false,true}do
 for _,wrapped in ipairs{false,true}do
  for _,mode in ipairs{'value','empty','initial-empty','syntax','error','invalid'}do
   local co=create(entry);if high then copy(exact,co)end
   push(co,mode,wrapped,high);local rc,status=resume(co,3);assert(rc==0 and status==0)
   local n,t=values(co);assert(n==7)
   print('reader-call-level',high,wrapped,mode,level(co),unpack(t,1,n));checked=checked+1
  end
 end
end
for _,native in ipairs{false,true}do
 local co=create(function()
  local me=boundary_thread();local visits=0
  local it=native and boundary_thread or function()visits=visits+1;return 1 end
  local ok,e=pcall(function()copy(exact,me);for _ in it do visits=visits+1;break end end)
  assert(not ok and visits==0 and level(me)==1 and e:find('C stack overflow',1,true))
  return ok,type(e),visits,level(me)
 end)
 assert(resume(co,0)==0);local n,t=values(co)
 print('iterator-call-level',native,n,unpack(t,1,n));checked=checked+1
end
print('reader-call-levels-complete',checked)
collectgarbage()
