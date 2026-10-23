local shared = {held=false, count=0}
local alias = shared
return Def.Quad {
    Name = 'InteractiveQuad',
    InitCommand = function(self)
        self:xy(32,48):zoomto(24,24):diffuse({1,0.25,0.5,0.75})
    end,
    OnCommand = function(self)
        SCREENMAN:GetTopScreen():AddInputCallback(function(event)
            if event.GameButton == 'Right' then
                if event.type == 'InputEventType_Release' then shared.held=false
                else shared.held=not alias.held end
                shared.count=alias.count+1
                self:visible(shared.held)
                return shared.count >= 3
            end
            return false
        end)
    end,
}
