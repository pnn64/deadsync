local recurse, recursetable
local actorpool, actorsofinterest = {}, {}
local function add(actor)
	if not actor then return end
	local rx, ry, rz = actor:getrotation()
	if actor:GetNumWrapperStates() == 0 then actor:AddWrapperState() end
	local actor = {
		actor,
		parent = actor:GetParent(),
		x = actor:GetX(),
		y = actor:GetY(),
		zoom = actor:GetZoom(),
		rx = rx,
		ry = ry,
		rz = rz,
		name = actor:GetName():lower(),
		texturepath = (actor.GetTexture and actor:GetTexture() and actor:GetTexture().GetPath and actor:GetTexture():GetPath()),
		text = actor.GetText and actor:GetText(),
		type = tostring(actor):lower(),
		ws = actor:GetWrapperState(1),
	}
	insert(actorpool, actor)
end
function recursetable(table)
	for i, v in pairs(table) do
		local s = tostring(v)
		if find(s, 'ActorFrame') or find(s, 'Player') or find(s, 'ActorScroller') or find(s, 'NoteField') then
			recurse(v)
		elseif find(s, 'table') then
			recursetable(v)
		else
			add(v)
		end
	end
end
function recurse(actor, func)
	if actor:GetNumChildren() <= 0 then
		add(v)
		return
	end
	for i, v in pairs(actor:GetChildren()) do
		local s = tostring(v)
		if find(s, 'ActorFrame') or find(s, 'Player') or find(s, 'ActorScroller') or find(s, 'NoteField') or find(s, 'PercentageDisplay') or find(s, 'StepsDisplay') then
			recurse(v)
		elseif find(s, 'table') then
			recursetable(v)
		else
			add(v)
		end
	end
	if func then func() end
end

return {recurse = recurse, actors = actorpool, interest = actorsofinterest}