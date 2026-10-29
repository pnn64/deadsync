-- Overwriting a C library's only string root invalidates its saved pointer.
-- The rooted mode retains that string until the loader has returned.
local retained={}
package.path=nil
setmetatable(package,{__index=function(t,k)
  assert(k=='path')
  if ROOT_REFERENCE_STRINGS then
    local _,value=debug.getlocal(2,3)
    retained[#retained+1]=value
  end
  assert(debug.setlocal(2,1,'changed-argument'))
  assert(debug.setlocal(2,3,'changed-name'))
  collectgarbage('collect')
  return 'absent-?.missing'
end})
local result=package.loaders[2]('original.name',{})
local expected='absent-original'..package.config:sub(1,1)..'name.missing'
assert(result:find(expected,1,true))
print('package-rooting-complete')
