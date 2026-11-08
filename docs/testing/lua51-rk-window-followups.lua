-- Entry-hook control and a remaining stack-view difference.
local function entry()return 42 end
debug.sethook(function(event)
  if event=='call'and debug.getinfo(2,'f').func==entry then
    local a,x=debug.getlocal(2,1)
    local b,y=debug.getlocal(2,2)
    print('entry',a,type(x),x,b,type(y),y)
  end
end,'c')
entry(17,19,23)
debug.sethook()

local visits,after=0
local function scalar()
  collectgarbage()
  local x=string.sub('abc',1,2)
  return x
end
debug.sethook(function()
  if debug.getinfo(2,'f').func==scalar then
    visits=visits+1
    if visits==1 then debug.setlocal(2,4,17)end
    if visits==3 then
      local _,v=debug.getlocal(2,4)
      after=type(v)..':'..tostring(v)
    end
  end
end,'',1)
scalar()
debug.sethook()
print('cleared',after)
