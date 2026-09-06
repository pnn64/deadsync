local duration = 60 / 140 * 0.3
local doors = Def.ActorFrame {}
for side = 1, 2 do
    doors[#doors + 1] = Def.Quad {
        Name = 'door' .. side,
        OnCommand = function(self)
            self:stretchto(0, 0, SCREEN_WIDTH, SCREEN_HEIGHT):diffusealpha(0)
            if side == 1 then self:cropright(0.5) else self:cropleft(0.5) end
        end,
        SlideDoorMessageCommand = function(self)
            self:x(side == 1 and 0 or SCREEN_WIDTH):diffusealpha(1)
                :linear(duration):x(SCREEN_CENTER_X)
        end,
    }
end
return doors
