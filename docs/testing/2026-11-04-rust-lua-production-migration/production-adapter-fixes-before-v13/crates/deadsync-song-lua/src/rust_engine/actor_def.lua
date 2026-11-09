-- Adapted from ITGmania v1.2.0, revision 5c737928d93778c2c9f68e276052b220b43a468f.
-- Original: Themes/_fallback/Scripts/02 ActorDef.lua.
-- Path separator/absolute-drive handling and DeadSync's existing virtual theme
-- actors are adapted here. Remaining decoration helpers have pending bindings.
-- See docs/testing/2026-11-04-rust-lua-actor-definitions.md for provenance.
local theme_path_actor_type = __songlua_theme_path_actor_type
local rawset, getinfo = rawset, debug.getinfo

-- File chunks normally carry @. The production compiler also uses bare
-- absolute authored names; its Def constructor recognizes those source paths.
local function DebugPathToRealPath( p )
    if not p then return nil end
    local Path
    if p:sub(1,1) == "@" then
        Path = p:sub(2)
    elseif p:match("^/") or p:match("^%a:[/\\]") then
        Path = p
    else
        return nil
    end
    local pos = Path:match("^.*()[/\\]") or 0
    return string.sub(Path, 1, pos)
end

local function MergeTables( left, right )
	local ret = { }
	for key, val in pairs(left) do
		ret[key] = val
	end

	for key, val in pairs(right) do
		if ret[key] then
			if type(val) == "function" and
			   type(ret[key]) == "function" then
				local f1 = ret[key]
				local f2 = val
				val = function(...)
					f1(...)
					return f2(...)
				end
			else
				Warn( string.format( "%s\n\nOverriding \"%s\": %s with %s",
					 debug.traceback(), key, type(ret[key]), type(val)) )
			end
		end

		ret[key] = val
	end
	setmetatable( ret, getmetatable(left) )
	return ret
end

DefMetatable = {
	__concat = function(left, right)
		return MergeTables( left, right )
	end
}

-- This is used as follows:
--
--   t = Def.Class { table }
Def = {}
setmetatable( Def, {
	__index = function(self, Class)
		-- t is an actor definition table.  name is the type
		-- given to Def.  Fill in standard fields.
		return function(t)
			if not ActorUtil.IsRegisteredClass(Class) then
				error( Class .. " is not a registered actor class", 2 )
			end

			t.Class = Class
			-- Explicit Def construction initializes resources again, even when
			-- given a declaration returned by the virtual theme loader.
			rawset(t, "__songlua_theme_path_actor", nil)

			local level = 2
			if t._Level then
				level = t._Level + 1
			end
			local info

			-- Sometimes info.source will be "=(tail call)" when we need the actual
			-- file something came from.
			-- So this loop climbs the stack until it reaches an actual file path
			-- or reaches end.
			repeat
				info = getinfo(level,"Sl");
				if info then
					-- Source file of caller:
					local source= info.source
					t._Source= source
					-- Line number of caller:
					t._Line = info.currentline
					t._Dir= DebugPathToRealPath(source)

					-- level is incremented for the next time through the loop.  Take
					-- that into account if level is used after the loop.
					level= level + 1
				end
			until t._Dir or not info

			setmetatable( t, DefMetatable )
			return t
		end
	end,
})

function ResolveRelativePath(path, level, optional)
	if path:sub(1,1) ~= "/" and not path:match("^%a:[/\\]") and path:sub(1,2) ~= "\\\\" then
		-- "Working directory":
		local info = getinfo(level+1,"S")
		local sDir = info and DebugPathToRealPath(info.source)
		-- A host invocation or tail call can hide the authored frame. The file
		-- loader retains its directory until that chunk finishes executing.
		if not sDir and type(__songlua_script_dir) == "string" and __songlua_script_dir ~= "" then
			sDir = __songlua_script_dir .. "/"
		end
		assert( sDir )

		path = sDir .. path
	end

	path = ActorUtil.ResolvePath(path, level+1, optional)
	return path
end

-- Load an actor template.
function LoadActorFunc( path, level )
	level = level or 1

	-- Virtual theme resources use the production host's media classification;
	-- they do not resolve relative to the caller or require a filesystem entry.
	local themeClass = theme_path_actor_type(path)
	if themeClass then
		return function()
			local actor = { _Level = level+1 }
			if themeClass == "Sound" then actor.File = path end
			if themeClass == "Sprite" then actor.Texture = path end
			actor = Def[themeClass](actor)
			actor.__songlua_theme_path_actor = true
			return actor
		end
	end

	if path == "" then
		error( "Passing in a blank filename is a great way to eat up RAM. Good thing we warn you about this." )
	end

	local ResolvedPath = ResolveRelativePath( path, level+1 )
	if not ResolvedPath then
		error( path .. ": not found", level+1 )
	end
	path = ResolvedPath

	local Type = ActorUtil.GetFileType( path )
	Trace( "Loading " .. path .. ", type " .. tostring(Type) )

	if Type == "FileType_Lua" then
		-- Load the file.
		local chunk, errmsg = loadfile( path )
		if not chunk then error(errmsg) end
		return chunk
	end

	if Type == "FileType_Bitmap" or Type == "FileType_Movie" then
		return function()
			return Def.Sprite {
				_Level = level+1,
				Texture = path
			}
		end
	elseif Type == "FileType_Sound" then
		return function()
			return Def.Sound {
				_Level = level+1,
				File = path
			}
		end
	elseif Type == "FileType_Model" then
		return function()
			return Def.Model {
				_Level = level+1,
				Meshes = path,
				Materials = path,
				Bones = path
			}
		end
	elseif Type == "FileType_Directory" then
		return function()
			return Def.BGAnimation {
				_Level = level+1,
				AniDir = path
			}
		end
	end

	error( path .. ": unknown file type (" .. tostring(Type) .. ")", level+1 )
end

-- Load and create an actor template.
function LoadActor( path, ... )
	local t = LoadActorFunc( path, 2 )
	assert(t)
	return t(...)
end

function WrapInActorFrame( t )
	return Def.ActorFrame { children = t }
end

NullActor = { Class="Actor", _Source="(null actor)" }

-- (c) 2006 Glenn Maynard
-- All rights reserved.
--
-- Permission is hereby granted, free of charge, to any person obtaining a
-- copy of this software and associated documentation files (the
-- "Software"), to deal in the Software without restriction, including
-- without limitation the rights to use, copy, modify, merge, publish,
-- distribute, and/or sell copies of the Software, and to permit persons to
-- whom the Software is furnished to do so, provided that the above
-- copyright notice(s) and this permission notice appear in all copies of
-- the Software and that both the above copyright notice(s) and this
-- permission notice appear in supporting documentation.
--
-- THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
-- OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
-- MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT OF
-- THIRD PARTY RIGHTS. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR HOLDERS
-- INCLUDED IN THIS NOTICE BE LIABLE FOR ANY CLAIM, OR ANY SPECIAL INDIRECT
-- OR CONSEQUENTIAL DAMAGES, OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS
-- OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR
-- OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR
-- PERFORMANCE OF THIS SOFTWARE.
