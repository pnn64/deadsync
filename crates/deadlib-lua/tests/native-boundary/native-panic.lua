local count=0
local function make(kind)
  local co=coroutine.create(function()
    if kind=='failed'then error('old')end
    if kind=='yielded'then coroutine.yield(11,13)end
    if kind=='partial'then return boundary_panic_yield_tail(11,13,17)end
  end)
  if kind~='fresh'then coroutine.resume(co)end
  return co
end
local first=make('fresh');local second=make('fresh')
assert(boundary_panic_swap(first,1)==4)
assert(boundary_panic_swap(second,-1)==1 and boundary_panic_swap(nil,2)==1)
assert(boundary_panic_swap(first,-1)==2 and boundary_panic_swap(second,0)==2)
assert(boundary_panic_swap(nil,-1)==0 and boundary_panic_swap(second,1)==0)
print('panic-shared',true);count=count+1
for _,kind in ipairs{'fresh','completed','yielded','failed','partial'}do
  for _,mode in ipairs{1,2}do
    for _,token in ipairs{{},'a\000b',17,false,newproxy(false)}do
      local co=make(kind);boundary_panic_swap(co,mode)
      local which,status,top,t,same,identity,caller,frame,after,new=boundary_panic_raise(co,token)
      assert(which==mode and status==2 and top==1 and same and identity and caller==0 and frame==0)
      assert(after==(mode==1 and 1 or 2)and new==2 and coroutine.status(co)=='dead')
      assert(boundary_panic_swap(nil,-1)==1)
      print(kind,mode,type(token),which,status,top,t,same,identity,caller,frame,after,new);count=count+1
      collectgarbage();collectgarbage()
    end
  end
end
do
  local co=make('fresh');boundary_panic_swap(co,1)
  local which,status,top,t,same,identity,caller,frame,after,new=boundary_panic_raise(co,nil)
  assert(which==1 and status==2 and top==1 and t==0 and same and identity and after==1 and new==2)
  print('nil-error',which,status,top,t,same,identity,after,new);count=count+1
end
do
  local exported
  local co=coroutine.create(function()local held={value=41};exported=function()return held.value end;coroutine.yield()end)
  assert(coroutine.resume(co));assert(exported()==41)
  debug.sethook(co,function()end,'cr',21)
  local token={};boundary_panic_raise(co,token)
  local hook,mask,n=debug.gethook(co);assert(type(hook)=='function'and mask=='cr'and n==21)
  collectgarbage();assert(exported()==41 and debug.getupvalue(exported,1)=='held')
  assert(debug.setupvalue(exported,1,{value=43})=='held'and exported()==43)
  print('closed-cell-hook',exported(),mask,n);count=count+1
end
do
  local token={};local ok,e=pcall(boundary_panic_protected_error,token)
  assert(not ok and e==token and boundary_panic_swap(nil,-1)==1)
  print('protected-error',ok,e==token);count=count+1
end
assert(boundary_panic_swap(nil,4)==1)
print('native-panic-complete',count)
