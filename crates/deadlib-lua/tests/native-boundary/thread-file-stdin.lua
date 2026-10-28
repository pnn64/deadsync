assert(io.read(1)=='7')
local count=0
for _,kind in ipairs{'fresh','completed','yielded','failed','partial'}do
  local co=coroutine.create(function()
    if kind=='failed'then error('old')end
    if kind=='yielded'then coroutine.yield(17)end
    if kind=='partial'then return boundary_thread_yield_tail(11,13,17)end
  end)
  if kind~='fresh'then local ok=coroutine.resume(co);assert(ok==(kind~='failed'))end
  local env=setmetatable({marker=53},{__index=_G});debug.setfenv(co,env)
  local s,b,a,old,new,result,slots=boundary_thread_file(co,nil)
  assert(s==0 and old==new and type(result)=='function'and slots[a]==result and getfenv(result)==env)
  assert(debug.getinfo(result,'S').source=='=stdin')
  local x,y=result();if kind=='fresh'then assert(x==53 and y==59)else assert(x==nil and y==nil)end
  assert(io.type(io.stdin)=='file'and io.read('*a')=='')
  print(kind,s,b,a,old,new,type(result),x,y);count=count+1
  collectgarbage()
end
print('thread-file-stdin-complete',count)
