local button = Var "Button"
return Def.ActorFrame {
    Def.Sprite {
        Texture=NOTESKIN:GetPath("_Down", "Core"),
        Frame0000=0,
        Delay0000=1,
    },
    Def.Sprite {
        Texture="_Electricity",
        Frames=Sprite.LinearFrames(16, 1),
        InitCommand=function(self)
            self:zoom(1.2):effectclock("timer"):SetAllStateDelays(0.05)
            if button == "Left" then
                self:setstate(0):rotationz(90)
            elseif button == "Down" then
                self:setstate(5):rotationz(0)
            elseif button == "Up" then
                self:setstate(8):rotationz(180)
            elseif button == "Right" then
                self:setstate(13):rotationz(90)
            end
        end,
    },
}
