local particle, active, alpha
local action = 1
local spawns = {4.75, 13.25}
return Def.ActorFrame{
    Def.Quad{
        InitCommand=function(self) particle = self self:visible(false):diffusealpha(0) end,
    },
    Def.Actor{
        OnCommand=function(self) self:queuecommand("Update") end,
        UpdateCommand=function(self)
            if active then
                alpha = alpha - 2.4 / 60
                particle:diffusealpha(math.max(0, alpha))
                if alpha < 0 then
                    particle:visible(false)
                    active = false
                end
            end
            while spawns[action] and GAMESTATE:GetSongBeat() >= spawns[action] do
                alpha, active = 1, true
                particle:diffusealpha(1):visible(true)
                action = action + 1
            end
            self:sleep(1/60):queuecommand("Update")
        end,
    },
}
