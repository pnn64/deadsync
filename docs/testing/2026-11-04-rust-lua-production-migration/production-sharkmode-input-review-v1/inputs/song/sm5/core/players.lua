-- player table, mostly set in ScreenReadyCommand
p = {
	c = {},
	j = {},
	ws = {},
	col = {},
}

-- redirects that allow a single player draw call to alternate to it's combo and judgments as needed
-- this is stupid
-- this should be automated
p.c.p = p
p.c.c = p.c
p.c.j = p.j
p.c.ws = p.ws
p.c.col = p.col

p.j.p = p
p.j.c = p.c
p.j.j = p.j
p.j.ws = p.ws
p.j.col = p.col

p.ws.p = p
p.ws.c = p.c
p.ws.j = p.j
p.ws.ws = p.ws
p.ws.col = p.col

p.col.p = p
p.col.c = p.c
p.col.j = p.j
p.col.ws = p.ws
p.col.col = p.col

local dummy = {}
dummy.__call = function() return setmetatable({}, dummy) end
dummy.__index = function() return setmetatable({}, dummy) end
dummy.__add = function() return 0 end
dummy.__sub = function() return 0 end
dummy.__mul = function() return 0 end
dummy.__div = function() return 0 end
dummy.__unm = function() return 0 end

setmetatable(
	p,
	{__call = function(t, pn)
		t[pn]:visible(true)
		t[pn]:Draw()
		t[pn]:visible(false)
		return t
	end,
	__index = dummy['__index']}
)
po = setmetatable({}, dummy)
pc = setmetatable({}, dummy)
setmetatable(
	p.c,
	{__call = function(t, pn)
		t.p[pn]:GetChild'Combo':GetWrapperState(1):visible(true)
		t.p[pn]:GetChild'Combo':Draw()
		t.p[pn]:GetChild'Combo':GetWrapperState(1):visible(false)
		return t.c
	end,
	__index = dummy['__index']}
)
setmetatable(
	p.j,
	{__call = function(t, pn)
		t.p[pn]:GetChild'Judgment':GetWrapperState(1):visible(true)
		t.p[pn]:GetChild'Judgment':Draw()
		t.p[pn]:GetChild'Judgment':GetWrapperState(1):visible(false)
		return t.j
	end,
	__index = dummy['__index']}
)

setmetatable(p.ws, dummy)
setmetatable(p.col, dummy)

local function setactor(pn)
	if p[pn] then
		po[pn] = GAMESTATE:GetPlayerState(pn - 1):GetPlayerOptions'ModsLevel_Song'
		pc[pn] = GAMESTATE:GetPlayerState(pn - 1):GetPlayerOptions'ModsLevel_Current' -- illegal, don't ever do this
		local notefield = p[pn]:GetChild'NoteField'

		p[pn]:fov(scalefov(45)) -- scale fov for proper z space spacing regardless of aspect
		p[pn]:vanishpoint(0, 0)
		-- p[pn]:visible(false)
		if p[pn]:GetNumWrapperStates() == 0 then p[pn]:AddWrapperState() end
		if notefield:GetNumWrapperStates() == 0 then notefield:AddWrapperState() end
		p.ws[pn] = notefield:GetWrapperState(1)
		p.ws[pn + 2] = p[pn]:GetWrapperState(1)
		
		p.c[pn] = p[pn]:GetChild'Combo'
		p.j[pn] = p[pn]:GetChild'Judgment'
		p.ws[pn]:xy(0, 0)
		p.ws[pn]:z(0)
		p.ws[pn]:zoom(1)
		p.ws[pn]:zoomz(1)
		p.ws[pn]:rotationx(0)
		p.ws[pn]:rotationy(0)
		p.ws[pn]:rotationz(0)

		p.ws[pn + 2]:xy(0, 0)
		p.ws[pn + 2]:z(0)
		p.ws[pn + 2]:zoom(1)
		p.ws[pn + 2]:zoomz(1)
		p.ws[pn + 2]:rotationx(0)
		p.ws[pn + 2]:rotationy(0)
		p.ws[pn + 2]:rotationz(0)

		p.col[pn] = notefield:GetColumnActors()
		
		-- store default player values and states
		player_defaults[pn] = {
			x = p[pn]:GetX(),
			y = p[pn]:GetY(),
			zoom = p[pn]:GetZoom(),
			zoomx = p[pn]:GetZoomX(),
			zoomy = p[pn]:GetZoomY(),
			notefield = {
				x = notefield:GetX(),
				y = notefield:GetY(),
				zoom = notefield:GetZoom(),
				zoomx = notefield:GetZoomX(),
				zoomy = notefield:GetZoomY(),
			}
		}
		
		-- sleep and reset methods because the editor might be stupid
		for i, v in ipairs{'Judgment', 'Combo'} do
			actor = p[pn]:GetChild(v)
			if actor:GetNumWrapperStates() == 0 then actor:AddWrapperState() end
			-- actor:GetWrapperState(1):xy(player_defaults[pn].x, player_defaults[pn].y)
			-- actor:GetWrapperState(1):visible(false)
			-- actor:sleep(9e9)
		end
	end
end

-- store default values of players
player_defaults = setmetatable({}, dummy)
return function()
	local we_are_in_a_stupid_editor = false 
	if find(screen:GetName(),"^ScreenEdit") and (not screen:GetChild'PlayerP1') then
		we_are_in_a_stupid_editor = true
	end
	print(tostring(we_are_in_a_stupid_editor))
	if we_are_in_a_stupid_editor then
		for i, v in pairs(screen:GetChild'') do
			if _G.tostring(v):find'Player' then
				p[1] = v
				setactor(1)
				player_defaults[1].x = sw > 640 and (scx + 480 / 2.25) or (sw * 0.75)
				p[1]:x(player_defaults[1].x)
			end
		end
	else
		for pn = 1, maximum_player_amount do
			p[pn] = screen:GetChild('PlayerP' .. pn) -- name players
			setactor(pn)
		end
	end
end