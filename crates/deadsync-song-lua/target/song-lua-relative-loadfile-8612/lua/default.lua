
local path = GAMESTATE:GetCurrentSong():GetSongDir() .. "lua/"
loadfile(path .. "helper.lua")()
return Def.ActorFrame {}
