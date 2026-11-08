-- Native publication workload: unchanged frame lists, with periodic authored
-- in-place edits. Uses a small tracked image and needs no GPU or audio device.
local sprites, tick = {}, 0
local texture = GAMESTATE:GetCurrentSong():GetSongDir() .. 'live_sprites.png'
local root = Def.ActorFrame {
    OnCommand = function(self)
        self:SetUpdateFunction(function()
            tick = tick + 1
            self:x(tick % 2)
            if tick % 120 == 0 then
                for _, sprite in ipairs(sprites) do
                    sprite.Frames[1].Delay = tick % 240 == 0 and 0.125 or 0.25
                end
            end
        end)
    end,
}
for i = 1, 256 do
    -- Sixteen animation states can share this image's single texture frame.
    local frames = {}
    for state = 1, 16 do frames[state] = {Frame=0, Delay=state/128} end
    root[#root + 1] = Def.Sprite {
        Name = 'Animated' .. i,
        Texture = texture,
        Frames = frames,
        InitCommand = function(self) sprites[#sprites + 1] = self end,
        OnCommand = function(self)
            self:xy((i - 1) % 16 * 32, math.floor((i - 1) / 16) * 24):zoom(0.125)
        end,
    }
end
return root
