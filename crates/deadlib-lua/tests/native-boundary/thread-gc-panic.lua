local count=0
local function make(kind)
  local co=coroutine.create(function()
    if kind=='failed'then error('old')end
    if kind=='yielded'then coroutine.yield(17)end
    if kind=='partial'then return boundary_execution_yield_tail(11,13,17)end
  end)
  if kind~='fresh'then local ok=coroutine.resume(co);assert(ok==(kind~='failed'))end
  return co
end
for _,kind in ipairs{'fresh','completed','yielded','failed','partial'}do
  for _,mode in ipairs{2,5}do
    for _,callback in ipairs{1,2}do
      for typ=1,5 do
        collectgarbage();collectgarbage('stop')
        local co=make(kind);local tokens={{},'GC failure',17,false};local token=tokens[typ]
        local visits=0;local exported
        do local p=newproxy(true);getmetatable(p).__gc=function()
          assert(coroutine.running()==co);visits=visits+1
          local held={value=71};exported=function()return held.value end
          if callback==2 then boundary_panic_swap(co,2)end
          error(token,0)
        end end
        local which,status,top,t,same,identity,caller,frame,after,new=boundary_gc_thread_panic(co,token,mode,2147483647)
        local expected_type=typ==1 and 5 or typ==2 and 4 or typ==3 and 3 or typ==4 and 1 or 0
        assert(which==callback and status==2 and top==1 and t==expected_type and same and identity)
        assert(caller==0 and frame==0 and after==(callback==2 and 2 or 1)and new==2 and visits==1)
        assert(coroutine.status(co)=='dead'and exported()==71)
        collectgarbage();assert(debug.setupvalue(exported,1,{value=73})=='held'and exported()==73)
        print('gc-panic',kind,mode,callback,type(token),which,status,top,t,same,identity,caller,frame,after,new,exported());count=count+1
      end
    end
  end
end
assert(count==100)
print('thread-gc-panic-complete',count)
