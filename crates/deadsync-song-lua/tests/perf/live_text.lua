-- Text publication with stable labels, changing counters and styled spans.
-- All behavior uses ordinary authored Lua APIs.
local labels, tick = {}, 0
local root = Def.ActorFrame {
    OnCommand = function(self)
        self:SetUpdateFunction(function()
            tick = tick + 1
            self:x(tick % 2)
            for i, label in ipairs(labels) do
                if i % 8 == 0 then
                    label:settextf('Counter %03d: %06d', i, tick)
                elseif tick % 60 == 0 then
                    label:settextf('Label %03d: page %d', i, math.floor(tick / 60))
                end
                if i % 2 == 0 and tick % 120 == 0 then
                    label:ClearAttributes()
                    label:AddAttribute(0, {Length=5, Diffuse={1,0.25,0.5,1}})
                    label:AddAttribute(6, {Length=-1, Glow={0.2,0.4,0.6,0.5}})
                end
            end
        end)
    end,
}
for i = 1, 128 do
    root[#root + 1] = Def.BitmapText {
        Name = 'Label' .. i,
        Font = 'Common normal',
        InitCommand = function(self)
            labels[#labels + 1] = self
            self:xy((i - 1) % 8 * 80, math.floor((i - 1) / 8) * 28):zoom(0.25)
                :settextf('Label %03d: steady text', i)
            if i % 2 == 0 then
                self:AddAttribute(0, {Length=5, Diffuse={0.25,0.5,1,1}})
                self:AddAttribute(6, {Length=3, Glow={0.1,0.2,0.3,0.25}})
            end
        end,
    }
end
return root
