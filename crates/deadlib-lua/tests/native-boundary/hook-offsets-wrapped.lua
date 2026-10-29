local m=m or assert(package.loadlib(assert(hook_stack_library),'luaopen_native'))('wrapped hook offsets')
local function pack(...)return {n=select('#',...),...}end
for _,event in ipairs{4,8,12} do
  for mode=24,26 do
    collectgarbage('stop')
    local wrapped=coroutine.wrap(function(x,y,z)return x,y,z end)
    local co=m.hook_stack_wrapped_thread(wrapped)
    assert(type(co)=='thread');m.hook_stack(co,event,mode)
    local r=pack(pcall(wrapped,17,29,42))
    print('first',event,mode,r.n,r[1],type(r[2]),rawequal(r[2],wrapped),coroutine.status(co))
    r=pack(pcall(wrapped))
    print('second',event,mode,r.n,unpack(r,1,r.n))
    print('state',event,mode,coroutine.status(co))
  end
end
print('wrapped-hook-offsets-complete')
