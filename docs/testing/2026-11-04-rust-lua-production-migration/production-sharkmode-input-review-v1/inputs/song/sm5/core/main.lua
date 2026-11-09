-- store data returned by core and modules, to run during ScreenReadyCommand
local ScreenReady = {}

local songposition = GAMESTATE:GetSongPosition()
globaloffset = PREFSMAN:GetPreference'GlobalOffsetSeconds'

-- load core functionality of template, certain files may return functions to be run later
loadfile'sm5/core/import.lua'
loadfile'sm5/core/constants.lua'

ScreenReady[#ScreenReady + 1] = loadfile'sm5/core/players.lua'
loadfile'sm5/core/modparser.lua'

-- load additional modules, not strictly necessary but complements usage of template
-- generic modules, always used regadless of file
module'helpers'
module'eases'
ScreenReady[#ScreenReady + 1] = module'scanactors'

-- load update loop and draw loop, this is where all the file specific content is stored
local update, draw
update, ScreenReady[#ScreenReady + 1] = loadfile'code/update.lua'[1], loadfile'code/update.lua'[2]
draw, ScreenReady[#ScreenReady + 1] = loadfile'code/draw.lua'[1], loadfile'code/draw.lua'[2]

-- set up our foreground
self:SetFOV(scalefov(45))
-- required in order for the foreground to not *poof* from existance
self:sleep(9e9)

self:addcommand("ScreenReady", function()
	-- variables that are only properly intialized at ScreenReady and later
	fg, target = self, quad
	beat = songposition:GetSongBeat()
	second_without_offsets = songposition:GetMusicSeconds()
	frame_without_offsets = floor(second_without_offsets * 60) -- assuming a 60fps update loop
	bps = songposition:GetCurBPS()
	bpm = bps * 60
	spb = 1 / bps
	globaloffset_frame = floor(globaloffset * 60)
	second = second_without_offsets + globaloffset
	frame = frame_without_offsets + globaloffset_frame
	local last_second = second
	local previous_frame = -9999
	is_new_frame = false
	deltatime = second - last_second
	players_drawn = false
	judgments_drawn = false
	combos_drawn = false

	-- run through all functions loaded by core or modules
	for i, func in ipairs(ScreenReady) do func() end
	
	-- hide screen elements
	-- for i, v in ipairs{'BPMDisplay', 'LifeFrame', 'ScoreFrame', 'Overlay', 'Underlay', 'ScoreP1', 'ScoreP2', 'LifeP1', 'LifeP2'} do
	-- 	if _G.SCREENMAN(v) then
	-- 		_G.SCREENMAN(v):hidden(1)
	-- 	end
	-- end

	-- update needs to take care of some extra steps
	self:SetUpdateFunction(function()
		-- update beat, second, and frames each tick
		beat = songposition:GetSongBeat()
		second_without_offsets = songposition:GetMusicSeconds()
		frame_without_offsets = floor(second_without_offsets * 60) -- assuming a 60fps update loop
		bps = songposition:GetCurBPS()
		bpm = bps * 60
		spb = 1 / bps
		second = second_without_offsets + globaloffset
		frame = frame_without_offsets + globaloffset_frame
		is_new_frame = frame ~= previous_frame
		deltatime = second - last_second
		players_drawn = false
		judgments_drawn = false
		combos_drawn = false
		update()
		m.apply()
		last_second = second
		previous_frame = frame
	end)
	self:SetDrawFunction(draw)
end)