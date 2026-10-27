local zero=boundary_newthread();local co
co=boundary_newthread(function()
 local current=boundary_thread()
 local ok,v=pcall(function()boundary_setlevel(zero,current);return boundary_yield(17)end)
 return ok,v
end)
local rc,status=boundary_resume(co,0);local n,t=boundary_thread_values(co)
print('lowered-yield-boundary',rc,status,n,type(t[1]),type(t[2]),coroutine.status(co))
