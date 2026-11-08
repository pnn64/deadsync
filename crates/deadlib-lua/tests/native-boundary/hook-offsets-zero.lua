local m=m or assert(package.loadlib(assert(hook_stack_library),'luaopen_native'))('zero hook offsets')
for _,event in ipairs{4,8,12} do
  for mode=24,26 do
    collectgarbage('stop')
    local co=coroutine.create(function(x,y,z)return x,y,z end)
    m.hook_stack(co,event,mode)
    print('first',event,mode,m.hook_stack_resume_window(co,17,29,42))
    print('zero',event,mode,m.hook_stack_move_signed(co,0))
    print('state',event,mode,m.waiting_snapshot(co))
    print('second',event,mode,m.hook_stack_resume_window(co))
    co=nil;collectgarbage('collect')
  end
end
print('zero-hook-offsets-complete')
