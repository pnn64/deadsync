local parent=coroutine.create(function()
 if WAITING_KIND=='failed'then error('old')end
 if WAITING_KIND=='yielded'then coroutine.yield(17)end
 if WAITING_KIND=='partial'then return boundary_execution_yield_tail(11,13,17)end
end)
if WAITING_KIND~='fresh'then local ok=coroutine.resume(parent);assert(ok==(WAITING_KIND~='failed'))end
boundary_panic_swap(parent,WAITING_PANIC)
local written=assert(io.open('waiting-exit-buffered-output.bin','wb'))
assert(written:write('waiting\0buffered\n'))
print('waiting-panic-exit',WAITING_KIND,WAITING_NATIVE,WAITING_PANIC)
boundary_unprotected_call(parent,function()
 local child=coroutine.create(function()boundary_panic_unhandled(parent,'waiting failure')end)
 if WAITING_NATIVE then boundary_waiting_c_resume(child)else coroutine.resume(child)end
 error('panic returned',0)
end,-1)
error('panic returned',0)
