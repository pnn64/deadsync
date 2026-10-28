local count=0
local types={[0]=4,4,5,7,6,4,4,4,4,3,4,8,0}
local tops={active=4,fresh=2,completed=1,yielded=1,failed=3,partial=1}
for _,kind in ipairs{'active','fresh','completed','yielded','failed','partial'}do
local selected=kind~='active'
for mode=0,12 do
  for _,fail in ipairs{false,true}do
    local co
    co=coroutine.create(function()
      collectgarbage();collectgarbage('stop')
      local target=selected and coroutine.create(function()
        if kind=='failed'then error('old')end
        if kind=='yielded'then coroutine.yield(17)end
        if kind=='partial'then return boundary_allocation_yield_tail(11,13,17)end
      end)or nil
      if selected and kind~='fresh'then local ok=coroutine.resume(target);assert(ok==(kind~='failed'))end
      local log={ran=false,after=false,fail=fail,context=false,parent='none'};local token={}
      do local p=newproxy(true);getmetatable(p).__gc=function()
        if not log.after then
          local info=debug.getinfo(2,'f')
          log.context=selected and coroutine.running()==target
            or not selected and not not(coroutine.running()==co and info and info.func==boundary_allocation_gc)
          if info then local f=info.func
            log.parent=f==boundary_allocation_gc and'allocation'or f==coroutine.yield and'yield'
              or f==boundary_allocation_yield_tail and'partial'or f==error and'error'or f==pcall and'pcall'or'other'
          end
        end
        log.ran=true;if log.fail then error(token,0)end
      end end
      local ok,ran,top,t
      if selected then ok,ran,top,t=pcall(boundary_allocation_gc,mode,token,log,target)
      else ok,ran,top,t=pcall(boundary_allocation_gc,mode,token,log)end
      log.after=true;log.fail=false
      local checked=mode<=7 or mode==11
      assert(log.context==checked)
      if fail then
        if selected and checked then assert(ok and ran and top==1 and t==5)
        else assert(not ok and ran==token)end
      else
        local expected=kind=='partial'and checked and 3 or tops[kind]
        assert(ok and ran==checked and top==expected and t==types[mode])
      end
      print('allocation-gc',kind,mode,fail,ok,ok and ran or ran==token,top,t,log.context,log.parent)
      collectgarbage();count=count+1
    end)
    local ok,e=coroutine.resume(co);assert(ok,tostring(e))
  end
end
end
print('allocation-gc-complete',count)
