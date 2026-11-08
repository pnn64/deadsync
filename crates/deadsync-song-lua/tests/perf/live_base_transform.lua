-- Authored immediate base transforms with active queues and getter feedback.
local actors, tick = {}, 0
local root = Def.ActorFrame {
    OnCommand = function(self)
        for i = 1, 128 do actors[i] = self:GetChild('Base' .. i) end
        self:SetUpdateFunction(function()
            tick = tick + 1
            local v = tick % 32 / 32
            for i, actor in ipairs(actors) do
                if tick % 32 == 1 then actor:linear(0.5):zoom(2):x(i+tick) end
                if tick % 32 == 17 then actor:stoptweening() end
                actor:basezoom(1+v):basezoomx(2+v):basezoomy(3+v):basezoomz(4+v)
                    :halign(v):valign(1-v)
                assert(actor:GetBaseZoomX() == 2+v)
                assert(actor:GetBaseZoomY() == 3+v and actor:GetBaseZoomZ() == 4+v)
                assert(actor:GetHAlign() == v and actor:GetVAlign() == 1-v)
                -- Tween interruption can leave a non-dyadic f32 scale. Lua's
                -- arithmetic uses doubles, while the native size getter uses floats.
                assert(math.abs(actor:GetZoomedWidth() - 8*((2+v)*actor:GetZoomX())) < 0.0001)
                assert(math.abs(actor:GetZoomedHeight() - 12*((3+v)*actor:GetZoomY())) < 0.0001)
            end
        end)
    end,
}
for i = 1, 128 do
    root[#root + 1] = Def.Quad {
        Name = 'Base' .. i,
        InitCommand = function(self) self:SetSize(8,12):xy(i % 16 * 30, math.floor(i / 16) * 40) end,
    }
end
return root
