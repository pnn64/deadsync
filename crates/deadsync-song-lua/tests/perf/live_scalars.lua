-- Generic scalar writes, destination/current reads and authored tween replacement.
local actors = {}
local root = Def.ActorFrame {
    OnCommand = function(self)
        for i = 1, 128 do actors[i] = self:GetChild('Scalar' .. i) end
        self:SetUpdateFunction(function()
            for i, actor in ipairs(actors) do
                actor.tick = actor.tick + 1
                local v = actor.tick % 32 / 32
                if actor.tick % 32 == 1 then actor:linear(0.5) end
                if actor.tick % 32 == 17 then actor:stoptweening() end
                actor:xy(i % 16 * 30 + v, math.floor(i / 16) * 40 + v)
                    :z(v):addx(2):addy(-1):addz(0.5):aux(v)
                    :zoom(1+v):zoomx(2+v):zoomy(3+v):zoomz(4+v)
                    :skewx(v):skewy(-v)
                    :cropleft(v/4):cropright(v/8):croptop(v/16):cropbottom(v/32)
                    :fadeleft(v/4):faderight(v/8):fadetop(v/16):fadebottom(v/32)
                assert(actor:GetDestX() == i % 16 * 30 + v + 2)
                assert(actor:GetDestY() == math.floor(i / 16) * 40 + v - 1)
                assert(actor:GetDestZ() == v + 0.5)
                assert(actor:GetZoom() == 2+v and actor:GetZoomX() == 2+v)
                assert(actor:GetZoomY() == 3+v and actor:GetZoomZ() == 4+v)
                assert(actor:GetZoomedWidth() == 8*(2+v))
                assert(actor:GetZoomedHeight() == 12*(3+v))
                if actor:GetTweenTimeLeft() == 0 then
                    assert(actor:GetX() == actor:GetDestX())
                    assert(actor:GetY() == actor:GetDestY())
                    assert(actor:GetZ() == actor:GetDestZ() and actor:getaux() == v)
                end
            end
        end)
    end,
}
for i = 1, 128 do
    root[#root + 1] = Def.Quad {
        Name = 'Scalar' .. i,
        InitCommand = function(self) self.tick = 0; self:SetSize(8,12) end,
    }
end
return root
