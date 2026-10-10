local states = {
    GAMESTATE:GetPlayerState(PLAYER_1),
    GAMESTATE:GetPlayerState(PLAYER_2),
}
local options = {
    states[1]:GetPlayerOptions("ModsLevel_Song"),
    states[2]:GetPlayerOptions("ModsLevel_Song"),
}
return Def.ActorFrame{
    OnCommand = function(self)
        for _, po in ipairs(options) do
            po:XMod(0, 9999)
            po:Bounce(0.75, 9999)
            po:BouncePeriod(0.5, 9999)
            po:BounceOffset(24, 9999)
            po:Tornado(1, 9999)
            po:TornadoPeriod(0.5, 9999)
            po:TornadoOffset(0.25, 9999)
        end
        local phase = 0
        self:SetUpdateFunction(function()
            local next_phase = math.floor(GAMESTATE:GetSongBeat())
            if next_phase == phase then return end
            phase = next_phase
            for _, po in ipairs(options) do
                if phase == 1 then
                    po:FromString("*9999 -50% Bounce, *9999 25% BouncePeriod, " ..
                        "*9999 -40% BounceOffset, *9999 50% TornadoPeriod, " ..
                        "*9999 100% TornadoOffset, *9999 2x")
                elseif phase == 2 then
                    po:FromString("clearall")
                elseif phase == 3 then
                    po:XMod(-0.5, 9999)
                    po:Bounce(-0.25, 9999)
                    po:BouncePeriod(-0.5, 9999)
                    po:BounceOffset(-12, 9999)
                    po:Tornado(-0.75, 9999)
                    po:TornadoPeriod(-0.5, 9999)
                    po:TornadoOffset(-0.5, 9999)
                end
            end
        end)
    end,
}
