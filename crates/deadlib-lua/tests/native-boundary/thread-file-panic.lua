local path=(THREAD_FILE_PREFIX or'thread-file')..'-panic.lua'
local count=0
local function observe(kind,mode,token,replacement)
  local exported
  local co=coroutine.create(function()
    if kind=='failed'then error('old')end
    if kind=='yielded'then coroutine.yield(17)end
    if kind=='partial'then return boundary_thread_yield_tail(11,13,17)end
  end)
  if kind~='fresh'then local ok=coroutine.resume(co);assert(ok==(kind~='failed'))end
  local f=assert(io.open(path,'wb'));assert(f:write('return 42'));assert(f:close())
  collectgarbage('stop')
  do local p=newproxy(true);getmetatable(p).__gc=function()
    assert(coroutine.running()==co)
    local held={value=71};exported=function()return held.value end
    if replacement then boundary_panic_swap(co,replacement)end
    assert(os.remove(path));error(token,0)
  end end
  boundary_panic_swap(co,mode)
  local which,status,top,t,same,identity,caller,frame,after,new=boundary_thread_file_panic(co,token,path)
  local expected=replacement or mode
  assert(which==expected and status==2 and top==1 and same and identity and caller==0 and frame==0)
  assert(after==(expected==1 and 1 or 2)and new==2 and coroutine.status(co)=='dead')
  assert(boundary_panic_swap(nil,-1)==1)
  collectgarbage();collectgarbage();assert(17+29==46)
  assert(exported()==71 and debug.getupvalue(exported,1)=='held')
  assert(debug.setupvalue(exported,1,{value=73})=='held'and exported()==73)
  if replacement then print('replaced',mode,replacement,which,status,top,t,same,identity,caller,frame,after,new)
  else print(kind,mode,type(token),which,status,top,t,same,identity,caller,frame,after,new)end
  count=count+1
end
for _,kind in ipairs{'fresh','completed','yielded','failed','partial'}do
  for _,mode in ipairs{1,2}do
    for _,box in ipairs{{{}},{'file token'},{17},{false},{}}do observe(kind,mode,box[1])end
  end
end
observe('fresh',0,{},1)
observe('yielded',1,{},2)
observe('partial',2,{},1)
assert(boundary_panic_swap(nil,4)==1)
print('thread-file-panic-complete',count)
