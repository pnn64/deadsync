return Def.ActorFrame{
    Def.ActorFrame{
        InitCommand=function(self) self:Center() end,
        Def.Sprite{
            Name="Fit",
            Texture="fit-rect.png",
            InitCommand=function(self)
                assert(self:GetWidth() == 64 and self:GetHeight() == 32)
                bg_fit_functions.BackgroundFitMode_CoverPreserve(self, SCREEN_WIDTH, SCREEN_HEIGHT)
                self:align(.5, 0):xy(0, -SCREEN_HEIGHT/2)
            end,
        },
    },
    Def.Quad{
        Name="Cover",
        InitCommand=function(self)
            self:FullScreen():diffusealpha(0)
            local fired = false
            self:SetUpdateFunction(function(self)
                if not fired and GAMESTATE:GetSongBeat() > 0 then
                    self:sleep(1):smooth(2):diffusealpha(1)
                    fired = true
                end
            end)
        end,
        FadeMessageCommand=function(self) self:sleep(1):smooth(2):diffusealpha(1) end,
    },
}
