local modules = ...

local max = math.max
local min = math.min
local sqrt = math.sqrt
local abs = math.abs

local InputSystem = {}

local held_buttons = {
	{},
	{}
}

local frame_count = 0

InputSystem.handle_input = function(event)
	local player_index = event.PlayerNumber == 'PlayerNumber_P2' and 2 or 1
	if event.type == "InputEventType_Release" then
		held_buttons[player_index][event.button] = nil
	elseif event.type == "InputEventType_FirstPress" or event.type == "InputEventType_Repeat" then
		held_buttons[player_index][event.button] = true
	end
end

InputSystem.update = function(ECS)
	frame_count = frame_count + 1
	if frame_count < 60 then
		return
	end

	for _, entity_id in ipairs(ECS.get_entity_ids_with_component('input')) do
		local entity = ECS.entities[entity_id]
		local player = entity.input.player
		local velocity = entity.physics.velocity

		local player_buttons = held_buttons[player]

		local dx = 0
		local dz = 0
		if player_buttons['Left'] then
			dx = dx - 1
		end
		if player_buttons['Right'] then
			dx = dx + 1
		end
		if player_buttons['Down'] then
			dz = dz - 1
		end
		if player_buttons['Up'] then
			dz = dz + 1
		end

		local magnitude = sqrt(dx*dx + dz*dz)
		if magnitude ~= 0 then
			dx = dx / magnitude
			dz = dz / magnitude
		end

		local speed = 0.3
		local accel = 0.01

		--[[
		if player_buttons['Select'] then
			velocity[2] = 0.1
			speed = 1
			accel = 0.05
		end
		]]--

		if entity.ball.slippy then
			accel = 0.003
		end

		local target_vx = speed*dx
		local vx = velocity[1]

		-- Don't slow the ball down if we're trying to go in the direction it's going.
		if target_vx == 0 or (target_vx > 0) ~= (vx > 0) or abs(target_vx) > abs(vx) then
			if vx < target_vx then
				vx = min(vx + accel, target_vx)
			else
				vx = max(vx - accel, target_vx)
			end
			velocity[1] = vx
		end

		local target_vz = speed*dz
		local vz = velocity[3]

		if target_vz == 0 or (target_vz > 0) ~= (vz > 0) or abs(target_vz) > abs(vz) then
			if vz < target_vz then
				vz = min(vz + accel, target_vz)
			else
				vz = max(vz - accel, target_vz)
			end
			velocity[3] = vz
		end
	end
end

return InputSystem