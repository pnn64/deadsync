module'draw'
local function vibrate(offset) return rand(frame - (offset or 0)) * 2 - 1 end

local actorstodraw = {}

local multipliers = module'multipliers'

local verts
local vertdiffuse = {full = {1, 1, 1, 1}, empty = {0, 0, 0, 0}}
do
	local log2 = log(2)
	local tw = exp(ceil(log(sw) / log2) * log2)
	local th = exp(ceil(log(sh) / log2) * log2)
	--In current-day OutFox, AFTs have same image & texture size, so we don't need (or want) a mult. -Kid
	local mult = _G.ProductFamily() == 'OutFox' and {x = 1, y = 1} or {x = sw / tw, y = sh / th}
	local txc = {
		x = {0 * mult.x, (((-10 * scaleh) / sw) + 0.5) * mult.x, (((10 * scaleh) / sw) + 0.5) * mult.x, 1 * mult.x},
		y = {0, 1 * mult.y}
	}
	verts = {
		{{0, 0, 0}, {1, 1, 1, 1}, {txc.x[1], txc.y[1]}}, -- top left
		{{0, sh, 0}, {1, 1, 1, 1}, {txc.x[1], txc.y[2]}}, -- bottom left
		{{scx - 10 * scaleh, 0, 0}, {1, 1, 1, 1}, {txc.x[2], txc.y[1]}}, -- top middle p1
		{{scx - 10 * scaleh, sh, 0}, {1, 1, 1, 1}, {txc.x[2], txc.y[2]}}, -- bottom middle p1
		{{scx + 10 * scaleh, 0, 0}, {1, 1, 1, 1}, {txc.x[3], txc.y[1]}}, -- top middle p2
		{{scx + 10 * scaleh, sh, 0}, {1, 1, 1, 1}, {txc.x[3], txc.y[2]}}, -- bottom middle p2
		{{sw, 0, 0}, {1, 1, 1, 1}, {txc.x[4], txc.y[1]}}, -- top right
		{{sw, sh, 0}, {1, 1, 1, 1}, {txc.x[4], txc.y[2]}}, -- bottom right
	}
end

local ratioview = false
return {function()
	target = quad
	xywh(scx, scy, sw, sh)
	hex'#000000'
	
	if ratioview then
		w(480 * (16 / 9))
		hex'#0f0000ff'
		draw()
		w(640)
		hex'#000f00ff'
		draw()
	end
	
	if range(73, 16) then
		players_drawn = true
		local kick = 0
		local amt = 32 * scaleh
		for i = 73, 79 do
			if range(i, 2) then
				kick = kick + pop(outback(a))
			end
		end
		for i = 81, 84, 0.5 do
			if range(i, 2) then
				kick = kick + pop(outback(a))
			end
		end
		if range(81, 4) then
			kick = kick * (1 - inoutsine(a))
		end
		-- draw()
		aft.texture:BeginRenderingTo()
		p(1)
		p(2)
		aft.texture:FinishRenderingTo()
		aft.finaltexture:BeginRenderingTo()
		if playersondifferentmults then
			local multp1, multp2 = multipliers.getmult(1), multipliers.getmult(2)
			local x = amt * kick
			local y = x / 12
			local xrm, xgm, xbm = x * vibrate(), x * vibrate(1000), x * vibrate(2000)
			local yrm, ygm, ybm = y * vibrate(3000), y * vibrate(4000), y * vibrate(5000)
			verts[1][1][1], verts[1][1][2] = xrm * multp1, yrm * multp1
			verts[2][1][1], verts[2][1][2] = xrm * multp1, yrm * multp1 + sh
			verts[3][1][1], verts[3][1][2] = xrm * multp1 + (scx - 10 * scaleh), yrm * multp1
			verts[4][1][1], verts[4][1][2] = xrm * multp1 + (scx - 10 * scaleh), yrm * multp1 + sh
			verts[5][1][1], verts[5][1][2] = xrm * multp2 + (scx + 10 * scaleh), yrm * multp2
			verts[6][1][1], verts[6][1][2] = xrm * multp2 + (scx + 10 * scaleh), yrm * multp2 + sh
			verts[7][1][1], verts[7][1][2] = xrm * multp2 + sw, yrm * multp2
			verts[8][1][1], verts[8][1][2] = xrm * multp2 + sw, yrm * multp2 + sh
			aft.output:diffuse(1, 0, 0, 1)
			aft.output:SetVertices(verts)
			aft.output:Draw()
			verts[1][1][1], verts[1][1][2] = xgm * multp1, ygm * multp1
			verts[2][1][1], verts[2][1][2] = xgm * multp1, ygm * multp1 + sh
			verts[3][1][1], verts[3][1][2] = xgm * multp1 + (scx - 10 * scaleh), ygm * multp1
			verts[4][1][1], verts[4][1][2] = xgm * multp1 + (scx - 10 * scaleh), ygm * multp1 + sh
			verts[5][1][1], verts[5][1][2] = xgm * multp2 + (scx + 10 * scaleh), ygm * multp2
			verts[6][1][1], verts[6][1][2] = xgm * multp2 + (scx + 10 * scaleh), ygm * multp2 + sh
			verts[7][1][1], verts[7][1][2] = xgm * multp2 + sw, ygm * multp2
			verts[8][1][1], verts[8][1][2] = xgm * multp2 + sw, ygm * multp2 + sh
			aft.output:diffuse(0, 1, 0, 1)
			aft.output:SetVertices(verts)
			aft.output:Draw()
			verts[1][1][1], verts[1][1][2] = xbm * multp1, ybm * multp1
			verts[2][1][1], verts[2][1][2] = xbm * multp1, ybm * multp1 + sh
			verts[3][1][1], verts[3][1][2] = xbm * multp1 + (scx - 10 * scaleh), ybm * multp1
			verts[4][1][1], verts[4][1][2] = xbm * multp1 + (scx - 10 * scaleh), ybm * multp1 + sh
			verts[5][1][1], verts[5][1][2] = xbm * multp2 + (scx + 10 * scaleh), ybm * multp2
			verts[6][1][1], verts[6][1][2] = xbm * multp2 + (scx + 10 * scaleh), ybm * multp2 + sh
			verts[7][1][1], verts[7][1][2] = xbm * multp2 + sw, ybm * multp2
			verts[8][1][1], verts[8][1][2] = xbm * multp2 + sw, ybm * multp2 + sh
			aft.output:diffuse(0, 0, 1, 1)
			aft.output:SetVertices(verts)
			aft.output:Draw()
		else
			local x = amt * kick * multipliers.getmastermult()
			local y = x / 12
			aft.output:diffuse(1, 0, 0, 1)
			aft.output:xy(x * vibrate(), y * vibrate(3000))
			aft.output:Draw()
			aft.output:xy(x * vibrate(1000), y * vibrate(4000))
			aft.output:diffuse(0, 1, 0, 1)
			aft.output:Draw()
			aft.output:xy(x * vibrate(2000), y * vibrate(5000))
			aft.output:diffuse(0, 0, 1, 1)
			aft.output:Draw()
		end
		aft.finaltexture:FinishRenderingTo()
		aft.finaloutput:Draw()
		screen:x(amt / 6 * vibrate() * kick)
	end

	if range(89, 32) then
		local kick = 0
		local amt = 48 * scaleh

		for i = 89, 99 do
			if range(i, 2) then kick = kick + pop(outback(a, 2)) end
		end
		if range(100, 1) then kick = kick + pop(outback(a, 2)) end
		if range(100.5, 0.5) then kick = kick + pop(outback(a, 2)) end
		if range(100.75, 0.5) then kick = kick + pop(outback(a, 2)) end
		if range(101, 1) then kick = kick + pop(outback(a, 2)) end
		if range(102, 1) then kick = kick + pop(outback(a, 2)) end
		if range(102.5, 1) then kick = kick + pop(outback(a, 2)) end

		if range(116.5, 0.5) then kick = kick + pop(outback(a, 2)) end
		if range(116.75, 0.5) then kick = kick + pop(outback(a, 2)) end
		if range(117, 0.5) then kick = kick + pop(outback(a, 2)) end
		if range(117.5, 1) then kick = kick + pop(outback(a, 2)) end
		if range(118, 1) then kick = kick + pop(outback(a, 2)) end
		if range(118.5, 1) then kick = kick + pop(outback(a, 2)) end

		for i = 105, 116 do
			if range(i, 2) then kick = kick + pop(outback(a, 2)) end
		end
		if playersondifferentmults then
			players_drawn = true
			draw()
			aft.texture:BeginRenderingTo()
			songbackground:Draw()
			for i, actor in ipairs(actorstodraw) do
				actor:Draw()
			end
			p(1)
			p(2)
			aft.texture:FinishRenderingTo()
			local multp1, multp2 = multipliers.getmult(1), multipliers.getmult(2)
			local x = amt * kick
			local y = x / 8
			local xm = x * vibrate(200)
			local ym = x * vibrate(5000)
			verts[1][1][1], verts[1][1][2] = xm * multp1, ym * multp1
			verts[2][1][1], verts[2][1][2] = xm * multp1, ym * multp1 + sh
			verts[3][1][1], verts[3][1][2] = xm * multp1 + (scx - 10 * scaleh), ym * multp1
			verts[4][1][1], verts[4][1][2] = xm * multp1 + (scx - 10 * scaleh), ym * multp1 + sh
			verts[5][1][1], verts[5][1][2] = xm * multp2 + (scx + 10 * scaleh), ym * multp2
			verts[6][1][1], verts[6][1][2] = xm * multp2 + (scx + 10 * scaleh), ym * multp2 + sh
			verts[7][1][1], verts[7][1][2] = xm * multp2 + sw, ym * multp2
			verts[8][1][1], verts[8][1][2] = xm * multp2 + sw, ym * multp2 + sh
			aft.output:diffuse(1, 1, 1, 1)
			aft.output:SetVertices(verts)
			aft.output:Draw()
		else
			local mult = multipliers.getmastermult()
			screen:xy(amt * vibrate(2000) * kick * mult, amt / 8 * vibrate(5000) * kick * mult)
		end
	end

	if range(120, 29) then
		players_drawn = true
		local kick = 0
		local amt = 48 * scaleh
		if range(120, 0.25) then kick = kick + pop(a) * 0.5 end
		if range(120.25, 0.125) then kick = kick + pop(a) * 2 end
		if range(120.5, 0.25) then kick = kick + pop(a) * 1.5 end
		for i = 121, 127 do
			if range(i, 2) then kick = kick + pop(outback(a, 2)) end
		end
		for i = 129, 130 do
			if range(i, 2) then kick = kick + pop(outback(a, 2)) end
		end
		for i = 131, 133 do
			if range(i, 0.25) then kick = kick + pop(a) * 1.5 end
		end
		-- right
		if range(127.5, 0.125) then kick = kick + pop(a) * 0.5 end
		if range(127.75, 0.5) then kick = kick + pop(a) * 1.5 end

		-- left
		if range(128.5, 0.125) then kick = kick + pop(a) * 0.5 end
		if range(128.75, 0.5) then kick = kick + pop(a) * 1.5 end

		if range(134.5, 0.125) then kick = kick + pop(a) * 2.5 end
		if range(134.75, 0.5) then kick = kick + pop(a) * 1.5 end
		if range(135, 1) then kick = kick + pop(outback(a)) * 2 end
		if range(135.5, 1) then kick = kick + pop(outback(a)) * 2 end
		if range(136, 1) then kick = kick + pop(outback(a)) * 2 end
		if range(136.5, 1) then kick = kick + pop(outback(a)) * 2 end
		for i = 137, 143 do
			if range(i, 2) then kick = kick + pop(outback(a, 2)) * 2 end
		end

		if range(143.5, 0.125) then kick = kick + pop(a) * 1.5 end
		if range(143.75, 0.25) then kick = kick + pop(a) * 1.5 end
		if range(144.5, 0.125) then kick = kick + pop(a) * 1.5 end
		if range(144.75, 0.25) then kick = kick + pop(a) * 1.5 end
		for i = 145, 148.5, 0.5 do
			if range(i, 1) then kick = kick + pop(outback(a, 2)) * 2 end
		end
		if range(145, 6) then
			kick = kick * (1 - inoutsine(a))
		end
		-- replace with background
		draw()
		aft.texture:BeginRenderingTo()
		songbackground:Draw()
		for i, actor in ipairs(actorstodraw) do
			actor:Draw()
		end
		p(1)
		p(2)
		aft.texture:FinishRenderingTo()
		aft.finaltexture:BeginRenderingTo()
		if playersondifferentmults then
			local multp1, multp2 = multipliers.getmult(1), multipliers.getmult(2)
			local x = amt * kick
			local y = x / 12
			local xrm, xgm, xbm = x * vibrate(), x * vibrate(1000), x * vibrate(2000)
			local yrm, ygm, ybm = y * vibrate(3000), y * vibrate(4000), y * vibrate(5000)
			verts[1][1][1], verts[1][1][2] = xrm * multp1, yrm * multp1
			verts[2][1][1], verts[2][1][2] = xrm * multp1, yrm * multp1 + sh
			verts[3][1][1], verts[3][1][2] = xrm * multp1 + (scx - 10 * scaleh), yrm * multp1
			verts[4][1][1], verts[4][1][2] = xrm * multp1 + (scx - 10 * scaleh), yrm * multp1 + sh
			verts[5][1][1], verts[5][1][2] = xrm * multp2 + (scx + 10 * scaleh), yrm * multp2
			verts[6][1][1], verts[6][1][2] = xrm * multp2 + (scx + 10 * scaleh), yrm * multp2 + sh
			verts[7][1][1], verts[7][1][2] = xrm * multp2 + sw, yrm * multp2
			verts[8][1][1], verts[8][1][2] = xrm * multp2 + sw, yrm * multp2 + sh
			aft.output:diffuse(1, 0, 0, 1)
			aft.output:SetVertices(verts)
			aft.output:Draw()
			verts[1][1][1], verts[1][1][2] = xgm * multp1, ygm * multp1
			verts[2][1][1], verts[2][1][2] = xgm * multp1, ygm * multp1 + sh
			verts[3][1][1], verts[3][1][2] = xgm * multp1 + (scx - 10 * scaleh), ygm * multp1
			verts[4][1][1], verts[4][1][2] = xgm * multp1 + (scx - 10 * scaleh), ygm * multp1 + sh
			verts[5][1][1], verts[5][1][2] = xgm * multp2 + (scx + 10 * scaleh), ygm * multp2
			verts[6][1][1], verts[6][1][2] = xgm * multp2 + (scx + 10 * scaleh), ygm * multp2 + sh
			verts[7][1][1], verts[7][1][2] = xgm * multp2 + sw, ygm * multp2
			verts[8][1][1], verts[8][1][2] = xgm * multp2 + sw, ygm * multp2 + sh
			aft.output:diffuse(0, 1, 0, 1)
			aft.output:SetVertices(verts)
			aft.output:Draw()
			verts[1][1][1], verts[1][1][2] = xbm * multp1, ybm * multp1
			verts[2][1][1], verts[2][1][2] = xbm * multp1, ybm * multp1 + sh
			verts[3][1][1], verts[3][1][2] = xbm * multp1 + (scx - 10 * scaleh), ybm * multp1
			verts[4][1][1], verts[4][1][2] = xbm * multp1 + (scx - 10 * scaleh), ybm * multp1 + sh
			verts[5][1][1], verts[5][1][2] = xbm * multp2 + (scx + 10 * scaleh), ybm * multp2
			verts[6][1][1], verts[6][1][2] = xbm * multp2 + (scx + 10 * scaleh), ybm * multp2 + sh
			verts[7][1][1], verts[7][1][2] = xbm * multp2 + sw, ybm * multp2
			verts[8][1][1], verts[8][1][2] = xbm * multp2 + sw, ybm * multp2 + sh
			aft.output:diffuse(0, 0, 1, 1)
			aft.output:SetVertices(verts)
			aft.output:Draw()
		else
			amt = amt * multipliers.getmastermult()
			aft.output:diffuse(1, 0, 0, 1)
			aft.output:xy(amt * vibrate() * kick, amt / 12 * vibrate(3000) * kick)
			aft.output:Draw()
			aft.output:xy(amt * vibrate(1000) * kick, amt / 12 * vibrate(4000) * kick)
			aft.output:diffuse(0, 1, 0, 1)
			aft.output:Draw()
			aft.output:xy(amt * vibrate(2000) * kick, amt / 12 * vibrate(5000) * kick)
			aft.output:diffuse(0, 0, 1, 1)
			aft.output:Draw()
		end
		aft.finaltexture:FinishRenderingTo()
		aft.finaloutput:Draw()
	end

	if range(256, 29) then
		players_drawn = true
		local kick = 0
		local amt = 64 * scaleh
		if range(256.5, 1) then kick = kick + pop(outback(a, 2)) end
		if range(263, 0.25) then kick = kick + pop(outback(a, 2)) end
		if range(263.25, 1) then kick = kick + pop(outback(a, 2)) end
		if range(264, 0.25) then kick = kick + pop(outback(a, 2)) end
		if range(264.25, 0.25) then kick = kick + pop(outback(a, 2)) end
		if range(264.5, 0.25) then kick = kick + pop(outback(a, 2)) * 4 end
		if range(264.75, 0.25) then kick = kick + pop(outback(a, 2)) * 2 end
		if range(265, 0.5) then kick = kick + pop(outback(a, 2)) end
		if range(268.5, 1) then kick = kick + pop(outback(a, 2)) end
		if range(273, 0.25) then kick = kick + pop(outback(a, 2)) end
		if range(273.25, 1) then kick = kick + pop(outback(a, 2)) end
		if range(273.75, 0.25) then kick = kick + pop(outback(a, 2)) end
		if range(274, 1) then kick = kick + pop(outback(a, 2)) end
		if range(276.5, 1) then kick = kick + pop(outback(a, 2)) end
		if range(279, 0.25) then kick = kick + pop(outback(a, 2)) end
		if range(279.25, 1) then kick = kick + pop(outback(a, 2)) end
		if range(280, 1) then kick = kick + pop(outback(a, 2)) end
		if range(280.5, 1) then kick = kick + pop(outback(a, 2)) * 4 end
		if range(284, 0.5) then kick = kick + pop(a) end
		if range(284.5, 0.5) then kick = kick + pop(a) end
		for i = 257, 262 do
			if range(i, 2) then kick = kick + pop(outback(a, 2)) end
		end
		for i = 266, 270 do
			if range(i, 2) then kick = kick + pop(outback(a, 2)) end
		end
		for i = 274, 278 do
			if range(i, 2) then kick = kick + pop(outback(a, 2)) end
		end
		for i = 281, 283 do
			if range(i, 2) then kick = kick + pop(outback(a, 2)) end
		end
		draw()
		aft.texture:BeginRenderingTo()
		songbackground:Draw()
		for i, actor in ipairs(actorstodraw) do
			actor:Draw()
		end
		p(1)
		p(2)
		aft.texture:FinishRenderingTo()
		aft.finaltexture:BeginRenderingTo()
		if playersondifferentmults then
			local multp1, multp2 = multipliers.getmult(1), multipliers.getmult(2)
			local mul = amt * kick
			local xrm, xgm, xbm = mul * vibrate(), mul * vibrate(1000), mul * vibrate(2000)
			local yrm, ygm, ybm = mul * vibrate(3000), mul * vibrate(4000), mul * vibrate(5000)
			verts[1][1][1], verts[1][1][2] = xrm * multp1, yrm * multp1
			verts[2][1][1], verts[2][1][2] = xrm * multp1, yrm * multp1 + sh
			verts[3][1][1], verts[3][1][2] = xrm * multp1 + (scx - 10 * scaleh), yrm * multp1
			verts[4][1][1], verts[4][1][2] = xrm * multp1 + (scx - 10 * scaleh), yrm * multp1 + sh
			verts[5][1][1], verts[5][1][2] = xrm * multp2 + (scx + 10 * scaleh), yrm * multp2
			verts[6][1][1], verts[6][1][2] = xrm * multp2 + (scx + 10 * scaleh), yrm * multp2 + sh
			verts[7][1][1], verts[7][1][2] = xrm * multp2 + sw, yrm * multp2
			verts[8][1][1], verts[8][1][2] = xrm * multp2 + sw, yrm * multp2 + sh
			aft.output:diffuse(1, 0, 0, 1)
			aft.output:SetVertices(verts)
			aft.output:Draw()
			verts[1][1][1], verts[1][1][2] = xgm * multp1, ygm * multp1
			verts[2][1][1], verts[2][1][2] = xgm * multp1, ygm * multp1 + sh
			verts[3][1][1], verts[3][1][2] = xgm * multp1 + (scx - 10 * scaleh), ygm * multp1
			verts[4][1][1], verts[4][1][2] = xgm * multp1 + (scx - 10 * scaleh), ygm * multp1 + sh
			verts[5][1][1], verts[5][1][2] = xgm * multp2 + (scx + 10 * scaleh), ygm * multp2
			verts[6][1][1], verts[6][1][2] = xgm * multp2 + (scx + 10 * scaleh), ygm * multp2 + sh
			verts[7][1][1], verts[7][1][2] = xgm * multp2 + sw, ygm * multp2
			verts[8][1][1], verts[8][1][2] = xgm * multp2 + sw, ygm * multp2 + sh
			aft.output:diffuse(0, 1, 0, 1)
			aft.output:SetVertices(verts)
			aft.output:Draw()
			verts[1][1][1], verts[1][1][2] = xbm * multp1, ybm * multp1
			verts[2][1][1], verts[2][1][2] = xbm * multp1, ybm * multp1 + sh
			verts[3][1][1], verts[3][1][2] = xbm * multp1 + (scx - 10 * scaleh), ybm * multp1
			verts[4][1][1], verts[4][1][2] = xbm * multp1 + (scx - 10 * scaleh), ybm * multp1 + sh
			verts[5][1][1], verts[5][1][2] = xbm * multp2 + (scx + 10 * scaleh), ybm * multp2
			verts[6][1][1], verts[6][1][2] = xbm * multp2 + (scx + 10 * scaleh), ybm * multp2 + sh
			verts[7][1][1], verts[7][1][2] = xbm * multp2 + sw, ybm * multp2
			verts[8][1][1], verts[8][1][2] = xbm * multp2 + sw, ybm * multp2 + sh
			aft.output:diffuse(0, 0, 1, 1)
			aft.output:SetVertices(verts)
			aft.output:Draw()
		else
			amt = amt * multipliers.getmastermult()
			aft.output:diffuse(1, 0, 0, 1)
			aft.output:xy(amt * vibrate() * kick, amt * vibrate(3000) * kick)
			aft.output:Draw()
			aft.output:xy(amt * vibrate(1000) * kick, amt * vibrate(4000) * kick)
			aft.output:diffuse(0, 1, 0, 1)
			aft.output:Draw()
			aft.output:xy(amt * vibrate(2000) * kick, amt * vibrate(5000) * kick)
			aft.output:diffuse(0, 0, 1, 1)
			aft.output:Draw()
		end
		aft.finaltexture:FinishRenderingTo()
		aft.finaloutput:Draw()
	end

	if range(286, 90) then
		players_drawn = true
		if runonce.finalblend then
			aft.finaloutput:blend'add'
		end
		for pn = 1, 2 do
			if rawget(p, pn) then
				local texture = aft['texturep' .. pn]
				local output = aft['outputp' .. pn]
				if runonce['blendp' .. pn] then
					output:blend'normal'
					output:xy(0, 0)
					output:diffuse(1, 1, 1, 1)
				end
				texture:BeginRenderingTo()
				draw()
				local mov = 0
				local offset = 0 - (as * 8) * scaleh
				local xmult = 1
				local mppsw = numplayfieldstorender % 2

				local distfromscx = player_defaults[pn].x - scx
				if range(286, 3, 90) then
					local q = 1 - outsine(a)
					mov = (player_defaults[pn].x * (1 - q) + scx * q) - player_defaults[pn].x + distfromscx + (as * 2 * mppsw)
				end
				for i = 1, numplayfieldstorender + 1 do
					local x = mod(((i -  1) * (64 * scaleh) * 4 + mov), ((numplayfieldstorender + 1) * (64 * scaleh) * 4)) - (numplayfieldstorender * (64 * scaleh) * 4 / 2)
					-- p[pn]:vanishpoint(-offset, 0)
					local segmentoffset = twoplayersactive and ((numplayfieldstorender - 1) * 256 * (pn - 1)) or 256
					p[pn]:x(scx + offset + x + segmentoffset)
					p(pn)
				end
				barrier:Draw()
				texture:FinishRenderingTo()
				if range(286, 3) then
					local t = outexpo(a)
					local q = outsine(a)
					output:diffusealpha(t)
					-- output:faderight(t)
					output:x(((player_defaults[pn].x * (1 - q) + scx * q)) - scx)
					p[pn]:x(player_defaults[pn].x)
					p(pn)
				end

				p[pn]:x(player_defaults[pn].x)
			end
		end
		if range(289, 80) then
			aft.finaltexture:BeginRenderingTo()
		end
		if twoplayersactive then
			-- aft.outputp1:cropright(0.5)
			if runonce.setverts then
				verts[1][2] = vertdiffuse.full
				verts[2][2] = vertdiffuse.full
				verts[3][2] = vertdiffuse.full
				verts[4][2] = vertdiffuse.full
				verts[5][2] = vertdiffuse.empty
				verts[6][2] = vertdiffuse.empty
				verts[7][2] = vertdiffuse.empty
				verts[8][2] = vertdiffuse.empty
				aft.outputp1:SetVertices(verts)
				verts[1][2] = vertdiffuse.empty
				verts[2][2] = vertdiffuse.empty
				verts[3][2] = vertdiffuse.empty
				verts[4][2] = vertdiffuse.empty
				verts[5][2] = vertdiffuse.full
				verts[6][2] = vertdiffuse.full
				verts[7][2] = vertdiffuse.full
				verts[8][2] = vertdiffuse.full
				aft.outputp2:SetVertices(verts)
				verts[1][2] = vertdiffuse.full
				verts[2][2] = vertdiffuse.full
				verts[3][2] = vertdiffuse.full
				verts[4][2] = vertdiffuse.full
				verts[5][2] = vertdiffuse.full
				verts[6][2] = vertdiffuse.full
				verts[7][2] = vertdiffuse.full
				verts[8][2] = vertdiffuse.full
			end
			aft.outputp1:Draw()
			-- aft.outputp2:cropleft(0.5)
			aft.outputp2:Draw()
		else
			barrier:diffuse(0, 0, 0, 0)
			if rawget(p, 1) then
				aft.outputp1:Draw()
			else
				aft.outputp2:Draw()
			end
		end
		if range(289, 80) then
			aft.finaltexture:FinishRenderingTo()
			local kick = 0
			local amt = 16 * scaleh

			for i = 289, 316 do
				if range(i, 2) then kick = kick + pop(outback(a, 2)) end
			end
			if range(304.5, 0.5) then kick = kick + pop(outback(a, 2)) * 2 end
			if range(304.75, 0.5) then kick = kick + pop(outback(a, 2)) * 2 end
			if range(321, 2) then kick = kick + pop(outquint(a)) * 4 end
			if range(325, 2) then kick = kick + pop(outquint(a)) * 4 end
			if range(337, 2) then kick = kick + pop(outquint(a)) * 4 end
			if range(341, 2) then kick = kick + pop(outquint(a)) * 4 end
			if range(353, 4) then kick = kick + pop(outquint(a)) * 12 end
			draw()
			if playersondifferentmults then
				local multp1, multp2 = multipliers.getmult(1), multipliers.getmult(2)
				local mul = amt * kick
				local xrm, xgm, xbm = mul * vibrate(), mul * vibrate(1000), mul * vibrate(2000)
				local yrm, ygm, ybm = mul * vibrate(3000), mul * vibrate(4000), mul * vibrate(5000)
				verts[1][1][1], verts[1][1][2] = xrm * multp1, yrm * multp1
				verts[2][1][1], verts[2][1][2] = xrm * multp1, yrm * multp1 + sh
				verts[3][1][1], verts[3][1][2] = xrm * multp1 + (scx - 10 * scaleh), yrm * multp1
				verts[4][1][1], verts[4][1][2] = xrm * multp1 + (scx - 10 * scaleh), yrm * multp1 + sh
				verts[5][1][1], verts[5][1][2] = xrm * multp2 + (scx + 10 * scaleh), yrm * multp2
				verts[6][1][1], verts[6][1][2] = xrm * multp2 + (scx + 10 * scaleh), yrm * multp2 + sh
				verts[7][1][1], verts[7][1][2] = xrm * multp2 + sw, yrm * multp2
				verts[8][1][1], verts[8][1][2] = xrm * multp2 + sw, yrm * multp2 + sh
				aft.finaloutput:diffuse(1, 0, 0, 1)
				aft.finaloutput:SetVertices(verts)
				aft.finaloutput:Draw()
				verts[1][1][1], verts[1][1][2] = xgm * multp1, ygm * multp1
				verts[2][1][1], verts[2][1][2] = xgm * multp1, ygm * multp1 + sh
				verts[3][1][1], verts[3][1][2] = xgm * multp1 + (scx - 10 * scaleh), ygm * multp1
				verts[4][1][1], verts[4][1][2] = xgm * multp1 + (scx - 10 * scaleh), ygm * multp1 + sh
				verts[5][1][1], verts[5][1][2] = xgm * multp2 + (scx + 10 * scaleh), ygm * multp2
				verts[6][1][1], verts[6][1][2] = xgm * multp2 + (scx + 10 * scaleh), ygm * multp2 + sh
				verts[7][1][1], verts[7][1][2] = xgm * multp2 + sw, ygm * multp2
				verts[8][1][1], verts[8][1][2] = xgm * multp2 + sw, ygm * multp2 + sh
				aft.finaloutput:diffuse(0, 1, 0, 1)
				aft.finaloutput:SetVertices(verts)
				aft.finaloutput:Draw()
				verts[1][1][1], verts[1][1][2] = xbm * multp1, ybm * multp1
				verts[2][1][1], verts[2][1][2] = xbm * multp1, ybm * multp1 + sh
				verts[3][1][1], verts[3][1][2] = xbm * multp1 + (scx - 10 * scaleh), ybm * multp1
				verts[4][1][1], verts[4][1][2] = xbm * multp1 + (scx - 10 * scaleh), ybm * multp1 + sh
				verts[5][1][1], verts[5][1][2] = xbm * multp2 + (scx + 10 * scaleh), ybm * multp2
				verts[6][1][1], verts[6][1][2] = xbm * multp2 + (scx + 10 * scaleh), ybm * multp2 + sh
				verts[7][1][1], verts[7][1][2] = xbm * multp2 + sw, ybm * multp2
				verts[8][1][1], verts[8][1][2] = xbm * multp2 + sw, ybm * multp2 + sh
				aft.finaloutput:diffuse(0, 0, 1, 1)
				aft.finaloutput:SetVertices(verts)
				aft.finaloutput:Draw()
			else
				amt = amt * multipliers.getmastermult()
				aft.finaloutput:diffuse(1, 0, 0, 1)
				aft.finaloutput:xy(amt * vibrate() * kick, amt * vibrate(3000) * kick)
				aft.finaloutput:Draw()
				aft.finaloutput:xy(amt * vibrate(1000) * kick, amt * vibrate(4000) * kick)
				aft.finaloutput:diffuse(0, 1, 0, 1)
				aft.finaloutput:Draw()
				aft.finaloutput:xy(amt * vibrate(2000) * kick, amt * vibrate(5000) * kick)
				aft.finaloutput:diffuse(0, 0, 1, 1)
				aft.finaloutput:Draw()
			end
		end
		
		--[[if range(286, 3) then
			local t = (1 - outexpo(a)) * 2
			local q = outsine(a)
			quad:fadeleft(t)
			quad:faderight(t)
			for pn = 1, 2 do
				if rawget(p, pn) then
					-- quad:x(player_defaults[pn].x * (1 - q) + scx * q)
					draw()
				end
			end
		else
			-- draw()
		end]]
	end
	
	-- if not judgments_drawn then p.j(1)(2) end
	-- if not combos_drawn then p.c(1)(2) end
	if not players_drawn then p(1)(2) end
end,
function()
	aft.base:SetSize(sw, sh):EnableAlphaBuffer(true):EnableFloat(true):EnableDepthBuffer(true):EnablePreserveTexture(true):Create()
	aft.texture = aft.base:GetTexture()
	aft.output:SetTexture(aft.texture):xy(0, 0):blend'add':SetDrawState{Mode = "DrawMode_QuadStrip", First = 1, Num = -1}:SetVertices(verts)
	
	aft.basep1 = aft.base
	aft.outputp1 = aft.output
	aft.texturep1 = aft.texture
	
	aft.basep2:SetSize(sw, sh):EnableAlphaBuffer(true):EnableFloat(true):EnableDepthBuffer(true):EnablePreserveTexture(true):Create()
	aft.texturep2 = aft.basep2:GetTexture()
	aft.outputp2:SetTexture(aft.texturep2):xy(0, 0):blend'add':SetDrawState{Mode = "DrawMode_QuadStrip", First = 1, Num = -1}:SetVertices(verts)

	aft.finalbase:SetSize(sw, sh):EnableAlphaBuffer(true):EnableFloat(true):EnableDepthBuffer(true):EnablePreserveTexture(true):Create()
	aft.finaltexture = aft.finalbase:GetTexture()
	aft.finaloutput:SetTexture(aft.finaltexture):xy(0, 0):SetDrawState{Mode = "DrawMode_QuadStrip", First = 1, Num = -1}:SetVertices(verts)

	for i, actor in pairs(SCREENMAN:GetTopScreen():GetChildren()) do
		local containsself = false
		if actor.GetChildren then
			for k, child in pairs(actor:GetChildren()) do
				if child == self then containsself = true break end
			end
		end
		if not containsself then
			insert(actorstodraw, actor)
		end
	end

	multipliers.updatemultipliers()
	twoplayersactive = (rawget(p, 1) and rawget(p, 2)) and true or false
	playersondifferentmults = (twoplayersactive and (multipliers.getmult(1) ~= multipliers.getmult(2))) or false
	numplayfieldstorender = twoplayersactive and ceil(ppsw / 2) or ppsw

	barrier:diffuse(0, 0, 0, 1):zoomto(20 * scaleh, sh):xy(scx, scy):fadeleft(0.5):faderight(0.5)
end}
