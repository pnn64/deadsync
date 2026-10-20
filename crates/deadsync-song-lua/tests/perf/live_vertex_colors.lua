-- Eight authored corner/edge writes per actor, with recurring tween destinations.
local actors = {}
local tick = 0
local root = Def.ActorFrame {
    OnCommand = function(self)
        self:SetUpdateFunction(function(frame)
            tick = tick + 1
            local phase = (tick % 16) / 16
            for _, actor in ipairs(actors) do
                if tick % 32 == 1 then actor:linear(0.5) end
                if tick % 32 == 17 then actor:stoptweening() end
                actor:diffuseupperleft(1, phase, 0.5, 0.75)
                    :diffuseupperright(phase, 0.5, 1, 0.5)
                    :diffuselowerleft(0.5, 1, phase, 0.25)
                    :diffuselowerright(1, 0.5, phase, 1)
                    :diffusetopedge(phase, 1, 0.5, 0.75)
                    :diffuserightedge(0.5, phase, 1, 0.5)
                    :diffusebottomedge(1, 0.5, phase, 0.25)
                    :diffuseleftedge(phase, 0.5, 1, 1)
            end
            -- Position smoke probe; full fingerprints include every corner color.
            frame:x(phase)
        end)
    end,
}
for i = 1, 128 do
    root[#root + 1] = Def.Quad {
        Name = 'VertexColor' .. i,
        OnCommand = function(self)
            actors[#actors + 1] = self
            self:xy(i % 16 * 32, math.floor(i / 16) * 32):zoomto(16, 16)
        end,
    }
end
return root
