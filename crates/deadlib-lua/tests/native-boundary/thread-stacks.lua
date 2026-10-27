do local co=coroutine.create(function()boundary_state()end);assert(coroutine.resume(co))end;collectgarbage()
local create,values,push,move=boundary_newthread,boundary_thread_values,boundary_thread_push,boundary_thread_move
local function top(co) local n=values(co);return n end
do
 local co=create();assert(type(co)=='thread' and coroutine.status(co)=='dead' and boundary_thread_status(co)==0 and top(co)==0)
 local entry=function(a)local b,c=coroutine.yield(a,{},nil);return b,c end
 push(co,entry);local n,t=values(co);assert(n==1 and t[1]==entry and coroutine.status(co)=='suspended')
 local ok,a,b,c=coroutine.resume(co,17);assert(ok and a==17 and type(b)=='table' and c==nil and top(co)==0)
 push(co,91,92);assert(top(co)==2)
 ok,a,b=coroutine.resume(co,18,19);assert(ok and a==18 and b==19 and top(co)==0 and coroutine.status(co)=='dead')
 push(co,function()return 42 end);assert(coroutine.status(co)=='suspended')
 assert(select(2,coroutine.resume(co))==42 and top(co)==0)
 print('thread-stack-states',n,ok,a,b,boundary_thread_status(co))
end
do
 local co=create(boundary_sum);local ok,result=coroutine.resume(co,17,25);assert(ok and result==42)
 local marker={};co=create();push(co,marker,function(a,b)collectgarbage();return a+b end)
 local ok,got,result=coroutine.resume(co,17,25);assert(ok and got==marker and result==42)
 print('thread-stack-entries',ok,result,top(co))
end
do
 local a,b=create(),create();local object,fn={},function()return 42 end
 push(a,nil,false,42,'a\0b',object,fn,a)
 local left,right=move(a,b,5);assert(left==2 and right==5)
 local n,t=values(b);assert(n==5 and t[1]==42 and t[2]=='a\0b' and t[3]==object and t[4]==fn and t[5]==a)
 left,right=move(b,a,0);assert(left==5 and right==2)
 left,right=move(a,a,8001);assert(left==2 and right==2 and boundary_thread_selfmove())
 print('thread-stack-transfer',n,left,right)
end
do
 local co=create()
 local zero,n,scalars,types,limits,registry,globals,identity,main=boundary_thread_primitives(co)
 assert(tonumber(zero)==0 and n==3 and scalars and types and limits and registry==debug.getregistry() and globals==_G and identity and not main)
 local count,t=values(co);assert(count==5 and t[1]==42 and t[2]==false and t[3]==nil and t[4]=='a\0b' and t[5]==zero)
 print('thread-stack-primitives',zero,n,count,scalars,types,limits,identity,main)
end
do
 local main=boundary_thread()
 local co=coroutine.create(function()local n,t=values(main);assert(n==1 and t[1]==coroutine.running());print('thread-stack-normal',n,type(t[1]))end)
 assert(coroutine.resume(co,17,29))
end
do
 local weak=setmetatable({},{__mode='v'})
 local co=create();do local object={};weak[1]=object;push(co,object)end
 collectgarbage();assert(weak[1]);local sink=create();move(co,sink,1);collectgarbage();assert(weak[1])
 local doomed=create();weak[2]=doomed;push(doomed,doomed);doomed=nil
 co=nil;sink=nil;collectgarbage();assert(weak[1]==nil and weak[2]==nil)
 print('thread-stack-lifetimes',weak[1]==nil,weak[2]==nil)
end
do
 local co=create();debug.setfenv(co,{thread_marker=42});push(co,1)
 -- Registry/global pseudo-index checks follow the selected thread's globals.
 local _,_,_,_,_,registry,globals=boundary_thread_primitives(co)
 assert(registry==debug.getregistry() and globals.thread_marker==42)
 print('thread-stack-globals',globals.thread_marker)
end
do
 local cases=assert(loadstring([[return {function(a)local b=42;error('bad')end,function()return nil+17 end,function()local t={};t:missing()end,function()error(17)end,function()error('bad',0)end}]],'=thread-stack-errors'))()
 for i,f in ipairs(cases)do
  local co=coroutine.create(f);local ok,message=coroutine.resume(co,29);assert(not ok)
  local n,t=values(co);local kinds={}
  assert(t[n]==message);if i==3 then assert(t[1]==t[3])end
  for j=1,n do local v=t[j];kinds[j]=type(v)..(type(v)=='number' and ':'..tostring(v)or type(v)=='string' and ':'..v or'')end
  print('thread-stack-failure',i,n,table.concat(kinds,','))
  collectgarbage();assert(top(co)==n)
 end
end
do
 local old=debug.getfenv(boundary_newthread);debug.setfenv(boundary_newthread,{own_environment=true})
 local co=create();debug.setfenv(boundary_newthread,old)
 local _,_,_,_,_,_,globals=boundary_thread_primitives(co);assert(globals==_G)
 debug.sethook(function()end,'l',17)
 local hooked=create(function()return 42 end);local f,mask,count=debug.gethook(hooked)
 debug.sethook(nil);assert(f==nil and mask=='l' and count==17 and select(2,coroutine.resume(hooked))==42)
 print('thread-stack-inheritance',globals==_G,mask,count)
end
collectgarbage()
do
 local co=create();local ok,n=boundary_thread_resize(co,8000);assert(ok and n==8000)
 ok,n=boundary_thread_resize(co,8001);assert(not ok and n==8000)
 assert(boundary_thread_resize(co,7));local n,t=values(co);assert(n==7 and next(t)==nil)
 assert(boundary_thread_resize(co,-3));assert(top(co)==5)
 assert(boundary_thread_resize(co,0) and coroutine.status(co)=='dead')
 print('thread-stack-bounds',n,top(co))
end
do
 local co=create();boundary_thread_primitives(co)
 local stable,text=boundary_thread_hold(co,4,function()
  local other=create();for i=1,1000 do push(other,'thread string '..i);if i%13==0 then collectgarbage()end end
 end)
 assert(stable and text=='a\0b');print('thread-stack-string',stable,#text)
end
do
 local checked=0
 for i=1,1000 do
  local a,b=create(),create();local object={i}
  push(a,object,a,nil,false,i);assert(move(a,b,5)==0)
  local n,t=values(b);assert(n==5 and t[1]==object and t[2]==a and t[3]==nil and t[4]==false and t[5]==i)
  assert(move(b,a,5)==0);assert(top(a)==5)
  if i%17==0 then collectgarbage()end;checked=checked+1
 end
 print('thread-stack-many',checked)
end
collectgarbage()
print('native-thread-stacks-complete',1000)
