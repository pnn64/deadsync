local fired = false
return Def.ActorFrame {
    InitCommand = function(self)
        self:SetUpdateFunction(function()
            if not fired and GAMESTATE:GetSongBeat() > 3 then
                fired = true
                self:playcommand("Fade")
                self:playcommand("Fade")
            end
        end)
    end,
    Def.Sprite {
        Name = "Fade",
        Texture = "fit-rect.png",
        OnCommand = function(self) self:xy(100, 120):diffusealpha(0.9) end,
        FadeCommand = function(self) self:linear(120 / 333):diffusealpha(0) end,
    },
}
