return Def.ActorFrame {
    Def.Sprite {
        Texture=NOTESKIN:GetPath("", "White"),
        InitCommand=function(self)
            self:diffuse(0.2,0.4,0.6,0.65):effectclock("beat"):rainbow():effectperiod(4)
        end
    },
    Def.Sprite {
        Texture=NOTESKIN:GetPath("", "White"),
        InitCommand=function(self) self:blend("BlendMode_Add"):diffusealpha(0) end,
        PressCommand=function(self) self:diffusealpha(0.2) end,
        LiftCommand=function(self) self:diffusealpha(0) end
    }
}
