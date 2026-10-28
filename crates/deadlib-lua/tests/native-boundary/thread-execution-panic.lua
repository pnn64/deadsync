local count=0
for _,kind in ipairs{'fresh','completed','yielded','failed','partial'}do
for _,method in ipairs{false,true}do
  local co=coroutine.create(function()
    if kind=='failed'then error('old')end
    if kind=='yielded'then coroutine.yield(17)end
    if kind=='partial'then return boundary_execution_yield_tail(11,13,17)end
  end)
  if kind~='fresh'then local ok=coroutine.resume(co);assert(ok==(kind~='failed'))end
  local token=method and{}or'attempt to concatenate a table value';local exported
  local x=setmetatable({},{__concat=method and function()
    assert(coroutine.running()==co)
    local held={value=71};exported=function()return held.value end
    error(token,0)
  end or nil})
  local which,status,top,t,same,identity,caller,frame,after,new=boundary_execution_concat_panic(co,token,x,'x')
  assert(which==1 and status==2 and top==1 and t==(method and 5 or 4)and same and identity and caller==0 and frame==0)
  assert(after==1 and new==2 and coroutine.status(co)=='dead')
  collectgarbage()
  if method then
    assert(exported()==71 and debug.getupvalue(exported,1)=='held')
    assert(debug.setupvalue(exported,1,{value=73})=='held'and exported()==73)
  end
  print(kind,method,which,status,top,t,same,identity,caller,frame,after,new,method and exported()or'plain');count=count+1
end
end
print('thread-execution-panic-complete',count)
