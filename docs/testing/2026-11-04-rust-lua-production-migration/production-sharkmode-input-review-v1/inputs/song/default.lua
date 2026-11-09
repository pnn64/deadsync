-- store important variables that we might use commonly from _G
local _ENV = {
	_G = _G,
	setmetatable = setmetatable,
	getmetatable = getmetatable,
	rawget = rawget,
	print = print,
	ipairs = ipairs,
	pairs = pairs,
	type = type,
	tonumber = tonumber,
	error = error,
	tostring = tostring,
	GAMESTATE = GAMESTATE,
	SCREENMAN = SCREENMAN,
	PREFSMAN = PREFSMAN,
	STATSMAN = STATSMAN,
	PlayerOptions = PlayerOptions,
	Def = Def,
}

_ENV.dir = _ENV.GAMESTATE:GetCurrentSong():GetSongDir()
_ENV.song = _ENV.GAMESTATE:GetCurrentSong()
-- cache return values of lua files so we avoid loading new instances if the same file is called multiple times
local filecache = {}
function _ENV.loadfile(path)
	if filecache[path] then
		return filecache[path]
	end
	local chunk, err = _G.loadfile(_ENV.dir .. path)
	-- error out with the correct file instead of pointing to this function definition
	if err then error(err, 2) return end
	if setfenv then setfenv(chunk, _ENV) end
	filecache[path] = chunk()
	return filecache[path]
end
function _ENV.module(module)
	return _ENV.loadfile('sm5/modules/' .. module .. '.lua')
end
_ENV._ENV = _ENV
local af = Def.ActorFrame {
	Name = "Mods",
	InitCommand = function(self)
		_ENV.self = self
		_ENV.loadfile'sm5/core/main.lua'
	end,
	OnCommand = function(self)
		_ENV.screen = _ENV.SCREENMAN:GetTopScreen()
		self:playcommand'ScreenReady'
	end,

	Def.Quad {Name = "quad"},
	Def.Quad {Name = "barrier"},
	Def.ActorFrameTexture {Name = "aft.base"},
	-- Def.Sprite {Name = "aft.output"},
	Def.ActorMultiVertex {Name = "aft.output"},
	
	Def.ActorFrameTexture {Name = "aft.basep2"},
	-- Def.Sprite {Name = "aft.outputp2"},
	Def.ActorMultiVertex {Name = "aft.outputp2"},
	
	Def.ActorFrameTexture {Name = "aft.finalbase"},
	-- Def.Sprite {Name = "aft.finaloutput"},
	Def.ActorMultiVertex {Name = "aft.finaloutput"},
	Def.Sprite {
		Name = "songbackground",
		InitCommand = function(self)
			local c = PREFSMAN:GetPreference'BGBrightness'
			self:LoadFromCurrentSongBackground():scale_or_crop_background():diffuse(c, c, c, 1)
		end
	}
}

return af