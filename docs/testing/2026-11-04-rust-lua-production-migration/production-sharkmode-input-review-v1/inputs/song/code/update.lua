local speeds = module'speedmods'
local grabactors = module'grabactors'
local multipliers = module'multipliers'

modsinuse = {}
for pn = 1, 2 do
	modsinuse[pn] = {
		-- Reverse = 0,
		-- Mini = 0,
		-- Dark = 0,
	}
end

speeds.setbpm(155)
local perspective = {{mod = "Overhead", percentage = 1, mult = 1}, {mod = "Overhead", percentage = 1, mult = 1}}
local perspectivemults = {Hallway = 0.75, Distant = 33 / 39, Incoming = 33 / 43, Space = 0.825}
local mini = {{percentage = 0, mult = 1}, {percentage = 0, mult = 1}}
local minimult = function(mini) return (2 - mini) / 2 end
local reverse = {{percentage = 0, flip = false}, {percentage = 0, flip = false}}
local isspacing = PlayerOptions.DrunkSpacing and 'spacing' or 'offset'

-- because some naughty actors call self:RemoveChild''
local function validateactor(actor) actor[1]:GetX() end
local function vibrate(offset) return rand(frame - (offset or 0)) * 2 - 1 end
local function hsflicker(t) if t == 1 then return 0 end return (floor(frame / 2)) % 2 end

return {function()
	-- actor sanity check, since it's possible that the theme removed some actors between us grabbing them and attempting to do something with them
	if runonce.validateactors then
		local rem = {}
		for i, actor in ipairs(grabactors.actors) do
			local ran, err = _G.pcall(validateactor, actor)
			if not ran then
				insert(rem, i)
			end
		end
		for i = #rem, 1, -1 do
			local staleactorpos = rem[i]
			remove(grabactors.actors, staleactorpos)
		end
	end

	if range(56, 1) or range(64, 1) or range(72, 1) or range(80, 1) then
		reset.plr.col'movex'
		for pn = 1, 2 do
			for c = 1, 4 do
				plr[pn].col.movex[c] = plr[pn].col.movex[c] + (vibrate(c * 1000) * 50 * pop(outquad(a))) * multipliers.getmult(pn)
			end
		end
		specific.plr.col'movex'
	end

	-- print(perspective[1].mod, perspective[2].mod)
	if range(233, 1, 12) then
		m('*9e9 ' .. insine(a) * -100 .. ' drawsizeback')
	end
	if range(246, 1) then
		m('*9e9 0 drawsizeback')
	end

	if range(47, 6) then
		local tiny, drunk = 0, 0
		if range(47, 2) then tiny = tiny + inexpo(tri(a)) * -75 drunk = drunk + inexpo(tri(a)) * 150 end
		if range(48, 2) then tiny = tiny + inexpo(tri(a)) * -75 drunk = drunk + inexpo(tri(a)) * -150 end
		if range(49, 2) then tiny = tiny + inexpo(tri(a)) * -75 drunk = drunk + inexpo(tri(a)) * 150 end
		if range(50, 2) then tiny = tiny + inexpo(tri(a)) * -75 drunk = drunk + inexpo(tri(a)) * -150 end

		for pn = 1, 2 do
			m[pn]('*9e9 ' .. tiny .. ' tiny, *9e9 ' .. drunk * multipliers.getmult(pn) .. ' drunk')
		end
	end

	if range(200, 17) then
		local tiny, drunk = 0, 0
		if range(200, 2) then tiny = tiny + inexpo(tri(a)) * -75 drunk = drunk + inexpo(tri(a)) * 150 end
		if range(201, 2) then tiny = tiny + inexpo(tri(a)) * -75 drunk = drunk + inexpo(tri(a)) * -150 end
		if range(202, 2) then tiny = tiny + inexpo(tri(a)) * -75 drunk = drunk + inexpo(tri(a)) * 150 end
		if range(203, 2) then tiny = tiny + inexpo(tri(a)) * -75 drunk = drunk + inexpo(tri(a)) * -150 end
		if range(204, 2) then tiny = tiny + inexpo(tri(a)) * -75 drunk = drunk + inexpo(tri(a)) * 150 end
		if range(205, 2) then tiny = tiny + inexpo(tri(a)) * -75 drunk = drunk + inexpo(tri(a)) * -150 end
		if range(206, 2) then tiny = tiny + inexpo(tri(a)) * -75 drunk = drunk + inexpo(tri(a)) * 150 end
		if range(207, 2) then tiny = tiny + inexpo(tri(a)) * -75 drunk = drunk + inexpo(tri(a)) * -150 end
		if range(208, 2) then tiny = tiny + inexpo(tri(a)) * -75 drunk = drunk + inexpo(tri(a)) * 150 end
		if range(209, 2) then tiny = tiny + inexpo(tri(a)) * -75 drunk = drunk + inexpo(tri(a)) * -150 end
		if range(210, 2) then tiny = tiny + inexpo(tri(a)) * -75 drunk = drunk + inexpo(tri(a)) * 150 end
		if range(211, 2) then tiny = tiny + inexpo(tri(a)) * -75 drunk = drunk + inexpo(tri(a)) * -150 end
		if range(212, 2) then tiny = tiny + inexpo(tri(a)) * -75 drunk = drunk + inexpo(tri(a)) * 150 end
		if range(213, 2) then tiny = tiny + inexpo(tri(a)) * -75 drunk = drunk + inexpo(tri(a)) * -150 end
		if range(214, 2) then tiny = tiny + inexpo(tri(a)) * -75 drunk = drunk + inexpo(tri(a)) * 150 end
		if range(215, 2) then tiny = tiny + inexpo(tri(a)) * -75 drunk = drunk + inexpo(tri(a)) * -150 end

		for pn = 1, 2 do
			m[pn]('*9e9 ' .. tiny .. ' tiny, *9e9 ' .. drunk * multipliers.getmult(pn) .. ' drunk')
		end
	end

	if range(66, 6) then
		local tiny = 0
		if range(66, 1) then tiny = tiny + pop(a) * -50 end
		if range(68, 1) then tiny = tiny + pop(a) * -50 end
		if range(70, 1) then tiny = tiny + pop(a) * -50 end

		m('*9e9 ' .. tiny .. ' tiny')
	end

	if range(103.5, 1) or range(119.5, 1) then
		local square = (flicker(a) * 400 - 200) * outexpo(tri(a))

		for pn = 1, 2 do
			m[pn]('*9e9 ' .. square * multipliers.getmult(pn) .. ' square')
		end
	end

	if range(271, 2) then
		local drunk = 0
		if range(271, 0.5, 2) then drunk = drunk + outexpo(a) * 200 end
		if range(271.5, 1, 2) then drunk = drunk - outexpo(a) * 200 end
		for pn = 1, 2 do
			m[pn]('*9e9 ' .. drunk * multipliers.getmult(pn) .. ' drunk, *9e9 ' .. drunk / 2 .. ' invert')
		end
	end

	if range(285, 8, 80) then
		for pn = 1, 2 do
			if rawget(p, pn) then
				local q = 1 - outexpo(a)
				po[pn]:Mini(mini[pn].percentage * q, 9e9)
				po[pn][perspective[pn].mod](po[pn], perspective[pn].percentage * q, 9e9)
				-- po[pn][]
				local original = speeds.speed[pn][1]
				local normalized = original * perspective[pn].mult * mini[pn].mult
				po[pn][speeds.speed[pn].func](po[pn], original * q + normalized * (1 - q) * 0.75)
			end
		end

		if range(317, 4, 80) then
			for pn = 1, 2 do
				if rawget(p, pn) then
					local original = speeds.speed[pn][1]
					local normalized = original * perspective[pn].mult * mini[pn].mult
					po[pn][speeds.speed[pn].func](po[pn], normalized * (0.75 + 0.25 * inoutsine(a)))
				end
			end
		end

		reset.col'movex''movey'
		reset.plr.col'movex''movey'
		for i = 289 - 0.5, 316, 4 do
			if range(i, 4) then
				if range(i, 1, 4) then
					a = inoutexpo(a)
					col.movex[1] = col.movex[1] + inoutsine(a) * 200
					col.movex[3] = col.movex[3] + inoutsine(a) * 200
					col.movey[1] = col.movey[1] + outsine(tri(a)) * -100
					col.movey[3] = col.movey[3] + outsine(tri(a)) * -100
				end
				if range(i + 2, 1, 4) then
					a = inoutexpo(a)
					col.movex[1] = col.movex[1] + inoutsine(a) * 200
					col.movex[3] = col.movex[3] + inoutsine(a) * 200
					col.movey[1] = col.movey[1] + outsine(tri(a)) * -100
					col.movey[3] = col.movey[3] + outsine(tri(a)) * -100
				end
				if range(i + 1, 1, 4) then
					a = inoutexpo(a)
					col.movex[2] = col.movex[2] + inoutsine(a) * 200
					col.movex[4] = col.movex[4] + inoutsine(a) * 200
					col.movey[2] = col.movey[2] + outsine(tri(a)) * -100
					col.movey[4] = col.movey[4] + outsine(tri(a)) * -100
				end
				if range(i + 3, 1, 4) then
					a = inoutexpo(a)
					col.movex[2] = col.movex[2] + inoutsine(a) * 200
					col.movex[4] = col.movex[4] + inoutsine(a) * 200
					col.movey[2] = col.movey[2] + outsine(tri(a)) * -100
					col.movey[4] = col.movey[4] + outsine(tri(a)) * -100
				end
			end
		end
		for pn = 1, 2 do
			if rawget(p, pn) then
				plr[pn].col.movex = plr[pn].col.movex + col.movex
				plr[pn].col.movey = plr[pn].col.movey + col.movey * reverse[pn].flip
			end
		end
		specific.plr.col'movex''movey'
	end

	if range(329, 9, 0) then
		local drunk = 0
		if range(329.5, 1) then
			drunk = drunk + pop(a) * 100
		end

		if range(335.5, 1) then
			drunk = drunk + pop(a) * -200
		end

		if range(330, 6) then
			local fb = beat - floor(beat)
			local sign = floor(beat) % 2 * 2 - 1
			drunk = drunk + outquint(tri(fb)) * 100 * sign
		end
		if range(336, 2) then drunk = drunk * (1 - outexpo(a)) end

		for pn = 1, 2 do
			local mult = multipliers.getmult(pn)
			m('*9e9 ' .. drunk * mult .. ' drunk, *9e9 ' .. drunk * mult / 2 .. ' tipsy, *9e9 -50 drunkperiod')
		end

		local kick = 0
		local amt = 32 * scaleh

		for i = 89, 99 do
			if range(i, 2) then kick = kick + pop(outback(a, 2)) end
		end
		if range(329.5, 1) then kick = kick + pop(outback(a, 2)) end
		if range(330, 2) then kick = kick + pop(outback(a, 2)) end
		if range(331, 2) then kick = kick + pop(outback(a, 2)) end
		if range(332, 2) then kick = kick + pop(outback(a, 2)) end
		if range(333, 2) then kick = kick + pop(outback(a, 2)) end
		if range(334, 2) then kick = kick + pop(outback(a, 2)) end
		if range(335, 1) then kick = kick + pop(outback(a, 2)) end
		if range(335.5, 1) then kick = kick + pop(outback(a, 2)) end

		screen:xy(amt * vibrate(2000) * kick, amt / 8 * vibrate(5000) * kick)
	end

	if range(346, 8) then
		local kick = 0
		local amt = 48 * scaleh

		for i = 89, 99 do
			if range(i, 2) then kick = kick + pop(outback(a, 2)) end
		end
		if range(346, 2) then kick = kick + pop(outback(a, 2)) end
		if range(347, 2) then kick = kick + pop(outback(a, 2)) end
		if range(347.667, 2) then kick = kick + pop(outback(a, 2)) end
		if range(348.333, 0.75) then kick = kick + pop(outback(a, 2)) * 2 end
		if range(348.667, 0.75) then kick = kick + pop(outback(a, 2)) * 2 end
		if range(349, 1) then kick = kick + pop(outback(a, 2)) end
		if range(349.667, 0.75) then kick = kick + pop(outback(a, 2)) * 2 end
		if range(350, 1) then kick = kick + pop(outback(a, 2)) end
		if range(350.667, 0.75) then kick = kick + pop(outback(a, 2)) * 2 end
		if range(351, 1) then kick = kick + pop(outback(a, 2)) end
		if range(351.667, 0.75) then kick = kick + pop(outback(a, 2)) * 2 end
		if range(352, 1) then kick = kick + pop(outback(a, 2)) end

		screen:xy(amt * vibrate(2000) * kick, amt / 4 * vibrate(5000) * kick)

		local drunk = 0

		if range(347.667, 1) then drunk = drunk + pop(a) * -100 end
		if range(348.333, 1) then drunk = drunk + pop(a) * 100 end
		if range(348.667, 1) then drunk = drunk + pop(a) * 100 end

		if range(346, 6) then
			local fb = beat - floor(beat)
			local sign = floor(beat) % 2 * 2 - 1
			drunk = drunk + outquint(tri(fb)) * 100 * sign
		end
		if range(352, 2) then drunk = drunk * (1 - outexpo(a)) end

		for pn = 1, 2 do
			local mult = multipliers.getmult(pn)
			m('*9e9 ' .. drunk * mult .. ' drunk, *9e9 ' .. drunk * mult / 2 .. ' tipsy, *9e9 -50 drunkperiod')
		end
	end
end,
function()
	-- fetch speedmods, convert to mmod if needed (cmod and xmod)
	speeds.grab(function(pn)
		local s = speeds.speed[pn]
		if s.type == 'c' then
			po[pn]:CMod(nil, 9e9, true):XMod(1, 9e9, true):MMod(s[1], 9e9)
			s.func = 'MMod'
		else
			po[pn]:XMod(1, 9e9, true)[s.func](po[pn], s[1], 9e9)
		end
	end)

	-- fetch the current perspective and associated multipliers to normalise the scroll speed when alternating perspectives
	for name, mult in pairs(perspectivemults) do
		for pn = 1, 2 do
			if rawget(p, pn) then
				local percent = (po[pn][name](po[pn]))
				if percent then
					perspective[pn].mod = name
					perspective[pn].percentage = percent
					perspective[pn].mult = mult
				end
			end
		end
	end

	-- fetch current mini and associated multiplier to normalise scroll speed when modifying mini percentage
	for pn = 1, 2 do
		if rawget(p, pn) then
			local percent = (po[pn]:Mini())
			mini[pn].percentage = percent
			mini[pn].mult = minimult(percent)

			local percent = (po[pn]:Reverse())
			reverse[pn].percentage = percent
			reverse[pn].flip = percent >= 0.5 and -1 or 1
		end
	end

	-- fetch Everything
	grabactors.recurse(screen, function()
		local interests = grabactors.interest
		for i, actor in ipairs(grabactors.actors) do
			actor.ws:xy(0, 0)
			actor.ws:z(0)
			actor.ws:zoom(1)
			actor.ws:zoomz(1)
			actor.ws:rotationx(0)
			actor.ws:rotationy(0)
			actor.ws:rotationz(0)
			if find(actor.name, 'Column') then -- column cues
				if not interests.columncues then interests.columncues = {} end
				insert(interests.columncues, actor)
			elseif find(actor.name, 'BPMDisplay') then -- bpm display
				if not interests.bpmdisplay then interests.bpmdisplay = {} end
				insert(interests.bpmdisplay, actor)
			elseif actor.text and find(actor.text, '%D') and not find(actor.text, '[\\.%%]') then -- any text containing characters
				if not interests.text then interests.text = {} end
				insert(interests.text, actor)
			elseif actor.text and find(actor.text, '%d') then -- numbers
				if not interests.numbers then interests.numbers = {} end
				insert(interests.numbers, actor)
			end
		end
	end)
end}