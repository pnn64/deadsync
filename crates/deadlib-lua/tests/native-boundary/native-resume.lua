do local co=coroutine.create(function()boundary_state()end);assert(coroutine.resume(co))end;collectgarbage()
local create,values,push,resize=boundary_newthread,boundary_thread_values,boundary_thread_push,boundary_thread_resize
local resume,yield=boundary_resume,boundary_yield
local function snapshot(co,label)
 local n,t=values(co);local kinds={}
 for i=1,n do local v=t[i];kinds[i]=type(v)..((type(v)=='string' or type(v)=='number' or type(v)=='boolean')and ':'..tostring(v)or'')end
 print(label,n,table.concat(kinds,','),boundary_thread_status(co),coroutine.status(co))
 return n,t
end
do
 local object={};local co=create(function(a,b)
  assert(a==object and b==17);collectgarbage()
  local c,d,e=yield(a,nil,false)
  assert(c==29 and d==nil and e==object)
  return c,d,e
 end)
 push(co,object,17);local rc,status=resume(co,2);assert(rc==1 and status==1)
 local n,t=snapshot(co,'native-resume-yield');assert(n==3 and t[1]==object and t[2]==nil and t[3]==false)
 -- Unpopped yield values precede the supplied arguments in the C return window.
 push(co,29,nil,object);rc,status=resume(co,3);assert(rc==0 and status==0)
 n,t=snapshot(co,'native-resume-return');assert(n==3 and t[1]==29 and t[2]==nil and t[3]==object)
 resize(co,0);push(co,boundary_sum,17,25);assert(resume(co,2)==0)
 n,t=values(co);assert(n==2 and t[1]==42 and t[2]==3)
 print('native-resume-reuse',n,t[1])
end
do
 local co=create();push(co,'prefix',function(a)return a+1 end,41)
 assert(resume(co,1)==0);local n,t=snapshot(co,'native-resume-prefix');assert(n==2 and t[1]=='prefix' and t[2]==42)
 local co=create(function(a)local b=coroutine.yield(a);return b end)
 push(co,17);assert(resume(co,1)==1);assert(select(2,coroutine.resume(co,42))==42)
 co=create(function()local a=yield(17);return a end)
 assert(select(2,coroutine.resume(co))==17);push(co,42);assert(resume(co,1)==0)
 n,t=values(co);assert(n==1 and t[1]==42)
 print('native-resume-mixed',t[1])
end
do
 local main=boundary_thread();local co
 co=create(function()
  local n,t=values(main);assert(n==2 and t[1]==co and t[2]==0)
  assert(coroutine.status(main)=='normal')
  local rc,status=resume(main,0);assert(rc==2 and status==0)
  n,t=values(main);assert(n==1 and t[1]=='cannot resume non-suspended coroutine')
  collectgarbage();return 42
 end)
 assert(resume(co,0)==0)
 local rc,status=resume(main,0);assert(rc==2 and status==0)
 print('native-resume-self',rc,status)
end
do
 local fs=assert(loadstring([[return {function(a)local b=42;error('bad')end,function()return nil+17 end,function()local t={};t:missing()end,function()error(17)end,function()error('bad',0)end}]],'=native-resume-errors'))()
 for i,f in ipairs(fs)do
  local co=create(f);push(co,29);local rc,status=resume(co,1);assert(rc==2 and status==2)
  local n,t=snapshot(co,'native-resume-error-'..i);assert(n>=2 and t[n]==t[n-1])
  rc,status=resume(co,0);assert(rc==2 and status==2)
  n,t=values(co);assert(n==1 and t[1]=='cannot resume non-suspended coroutine')
 end
end
do
 local co=create(17);assert(resume(co,0)==2)
 local n,t=snapshot(co,'native-resume-invalid-entry');assert(t[n]=='attempt to call a number value' and t[n]==t[n-1])
 local callable=setmetatable({},{__call=function(self,a)return self,a end})
 co=create(callable);push(co,42);assert(resume(co,1)==0)
 n,t=values(co);assert(n==2 and t[1]==callable and t[2]==42)
 print('native-resume-callable',n,t[2])
end
for _,yield in ipairs{yield,coroutine.yield}do
 local co=create(function()return yield('old',nil,false)end)
 assert(resume(co,0)==1)
 local name,a=debug.getlocal(co,0,1);local name3,c=debug.getlocal(co,0,3)
 assert(name=='(*temporary)' and a=='old' and name3==name and c==false and debug.getlocal(co,0,4)==nil)
 assert(debug.setlocal(co,0,1,'changed')==name and debug.setlocal(co,0,4,42)==name)
 local n,t=values(co);assert(n==3 and t[1]=='changed' and t[2]==nil and t[3]==false)
 resize(co,0);assert(debug.getlocal(co,0,1)==nil and debug.setlocal(co,0,1,42)==name)
 assert(resume(co,0)==0 and values(co)==0)
 print('native-resume-suspended-locals',n,t[1],t[3])
end
for _,yield in ipairs{yield,coroutine.yield}do
 local co=create(function()return yield('old')end)
 local seen=0
 debug.sethook(co,function(event)
  if event=='return' and debug.getinfo(2,'f').func==yield then
   local n1,a=debug.getlocal(2,1);local n2,b=debug.getlocal(2,2)
   local n3,c=debug.getlocal(2,3);local n4,d=debug.getlocal(2,4)
   assert(n1=='(*temporary)' and n2==n1 and n3==n1 and n4==n1)
   assert(a=='old' and b==17 and c==25 and type(d)=='table' and debug.getlocal(2,5)==nil)
   debug.setlocal(2,2,19);collectgarbage();seen=seen+1
  end
 end,'r')
 assert(resume(co,0)==1);push(co,17,25);assert(resume(co,2)==0)
 local n,t=values(co);assert(n==2 and t[1]==19 and t[2]==25 and seen==1)
 debug.sethook(co,nil);print('native-resume-return-hook',seen,n,t[1],t[2])
end
do
 local parent=create(function()
  local child=create(function(a)local b=yield(a);return b end)
  push(child,17);assert(resume(child,1)==1);resize(child,0);push(child,42)
  assert(resume(child,1)==0);local n,t=values(child);assert(n==1 and t[1]==42)
  local a=yield(t[1]);return a
 end)
 assert(resume(parent,0)==1);resize(parent,0);push(parent,29);assert(resume(parent,1)==0)
 local n,t=values(parent);assert(n==1 and t[1]==29)
 print('native-resume-nested',n,t[1])
end
do
 local message='attempt to yield across metamethod/C-call boundary'
 local ok,error=pcall(yield,17);assert(not ok and error==message)
 local co=create(function()return pcall(yield,17)end);assert(resume(co,0)==0)
 local n,t=values(co);assert(n==2 and t[1]==false and t[2]==message)
 co=create(function()return setmetatable({},{__add=function()return yield(17)end})+1 end)
 assert(resume(co,0)==2);n,t=values(co);assert(t[n]==message)
 print('native-resume-boundaries',ok,n,t[n])
end
do
 local weak=setmetatable({},{__mode='v'})
 local co=create(function()local v={};weak[1]=v;yield(v);return 42 end)
 assert(resume(co,0)==1);collectgarbage();assert(weak[1])
 resize(co,0);assert(resume(co,0)==0);collectgarbage();assert(weak[1]==nil)
 print('native-resume-roots',weak[1]==nil)
end
collectgarbage()
do
 local checked=0
 for i=1,1000 do
  local co=create(function(a)local b=yield(a);return b end)
  push(co,i);assert(resume(co,1)==1);resize(co,0);push(co,i+1);assert(resume(co,1)==0)
  local n,t=values(co);assert(n==1 and t[1]==i+1);checked=checked+1
  if i%13==0 then collectgarbage()end
 end
 print('native-resume-complete',checked)
end
collectgarbage()
