local screen = SCREENMAN:GetTopScreen()
local phase = -1
return Def.ActorFrame {
    OnCommand = function(self)
        self:SetUpdateFunction(function(self)
            local next = math.floor(GAMESTATE:GetCurMusicSeconds())
            if next == phase then return end
            phase = next
            screen:xy(80, 40):z(0):rotationx(0):rotationy(0):rotationz(0):zoom(1)
            if phase == 0 then
                screen:rotationx(17)
            elseif phase == 1 then
                screen:rotationy(-11):zoom(0.8)
            elseif phase == 2 then
                screen:z(24)
            else
                screen:rotationx(17):zoom(0):z(24)
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
            OnCommand = function(self)
                self:xy(40, 20):z(70):zoomtowidth(60):zoomtoheight(30)
            end
        }
    }
}
