local old=ResolveRelativePath
return function(path,level,optional)
 local base=__songlua_script_dir
 if path:sub(1,1)~='/' and not path:match('^%a:[/\\]') and path:sub(1,2)~='\\\\' and type(base)=='string' and base~='' then
  Warn('DIRECTORY_PROBE '..path..' base='..base)
  path=base..'/'..path
 end
 local result=old(path,level+1,optional);return result
end