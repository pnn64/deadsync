-- Immediate render flags while tween queues are active. No chart-specific host API.
local actors, tick = {}, 0
local root = Def.ActorFrame {
    OnCommand = function(self)
        for i = 1, 128 do actors[i] = self:GetChild('Flags' .. i) end
        self:SetUpdateFunction(function(frame)
            tick = tick + 1
            frame:SetDrawByZPosition(tick % 2 == 0)
            for i, actor in ipairs(actors) do
                local enabled = (tick + i) % 2 == 0
                if tick % 32 == 1 then actor:linear(0.5):x(i + tick) end
                if tick % 32 == 17 then actor:stoptweening() end
                actor:zbuffer(enabled):ztestmode(enabled and 'WriteOnFail' or 'Off')
                    :zwrite(not enabled):SetTextureFiltering(enabled)
                    :texturewrapping(not enabled):MaskSource():MaskDest()
            end
        end)
    end,
}
for i = 1, 128 do
    root[#root + 1] = Def.Quad {
        Name = 'Flags' .. i,
        InitCommand = function(self) self:zoomto(8,12):xy(i % 16 * 30, math.floor(i / 16) * 40) end,
    }
end
return root
