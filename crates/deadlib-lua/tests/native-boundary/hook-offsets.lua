local m=m or assert(package.loadlib(assert(hook_stack_library),'luaopen_native'))('hook offsets')
local function pack(...)return {n=select('#',...),...}end
local function show(label,event,mode,...)
  local r=pack(...);local out={label,event,mode,r.n}
  for i=1,r.n do local v=r[i];out[#out+1]=v==nil and 'nil' or v=='' and '<empty>' or tostring(v)end
  print(unpack(out))
end
for _,event in ipairs{4,8,12} do
  for mode=24,26 do
    collectgarbage('stop')
    local co=coroutine.create(function(x,y,z)return x,y,z end)
    m.hook_stack(co,event,mode)
    show('raw-first',event,mode,m.hook_stack_resume_window(co,17,29,42))
    show('raw-state',event,mode,m.waiting_snapshot(co))
    show('raw-second',event,mode,m.hook_stack_resume_window(co))
    show('raw-done',event,mode,m.waiting_snapshot(co))
    co=coroutine.create(function(x,y,z)return x,y,z end)
    m.hook_stack(co,event,mode)
    show('move-first',event,mode,m.hook_stack_resume_window(co,17,29,42))
    show('move',event,mode,pcall(m.hook_stack_move_signed,co,23-mode))
    show('move-state',event,mode,m.waiting_snapshot(co))
    co=nil;collectgarbage('collect')
  end
end
for _,event in ipairs{1,2,4,8,12} do
  for mode=27,29 do
    collectgarbage('stop')
    local co=coroutine.create(function(x,y,z)return x,y,z end)
    local other=coroutine.create(function()end)
    m.hook_stack(co,event,mode,nil,other)
    show('active',event,mode,coroutine.resume(co,17,29,42))
    show('active-top',event,mode,debug.getregistry()['native.hook.stack.active_top'])
    show('destination',event,mode,m.waiting_snapshot(other))
    collectgarbage('collect')
  end
end
print('hook-offsets-complete')
