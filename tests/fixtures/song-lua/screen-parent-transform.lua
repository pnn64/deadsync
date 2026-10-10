local screen = SCREENMAN:GetTopScreen()
local phase = -1
return Def.ActorFrame {
    OnCommand = function(self)
        self:SetUpdateFunction(function(self)
            local next = math.floor(GAMESTATE:GetCurMusicSeconds())
            if next == phase then return end
            phase = next
            if phase == 0 then
                screen:x(80):y(40):rotationz(15):zoom(0.8)
            elseif phase == 1 then
                screen:x(100):y(-20):rotationz(-15):zoom(-0.6)
            elseif phase == 2 then
                screen:x(0):y(0):rotationz(0):zoom(0)
            else
                screen:x(30):y(20):rotationz(12):zoom(1.2):fov(43)
            end
        end)
    end,
    Def.Quad {
        OnCommand = function(self)
            self:xy(427, 240):zoomtowidth(140):zoomtoheight(90)
        end
    },
    Def.ActorFrame {
        OnCommand = function(self) self:xy(150, 80):rotationz(-10):zoom(0.7) end,
        Def.Quad {
            OnCommand = function(self) self:xy(40, 20):zoomtowidth(60):zoomtoheight(30) end
        }
    }
}
