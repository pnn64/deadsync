-- Isolate 7th Gear's paired Mini/XMod pulse. The reciprocal speed keeps
-- note spacing constant while the receptors and arrows change size.
local options = GAMESTATE:GetPlayerState(0):GetPlayerOptions('ModsLevel_Song')
return Def.ActorFrame{
    OnCommand = function(self)
        self:SetUpdateFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            if beat >= 3 and beat < 4 then
                options:FromString('*10000 -40 mini, *10000 0.8333333333333333x')
                return
            end
            local zoom = beat >= 1 and beat < 2 and 1.2 or 1
            options:Mini((1 - zoom) * 2, 10000)
            options:XMod(1 / zoom, 10000)
        end)
    end,
}
