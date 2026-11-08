-- Captured texture changes and repeated message names exercise offline finalization.
local sprites, judgments = {}, {}
local phase = -1
local root = Def.ActorFrame{
    Name = 'FinalizationEvents',
    OnCommand = function(self)
        for i, name in ipairs({'PlayerP1', 'PlayerP2'}) do
            local judgment = SCREENMAN:GetTopScreen():GetChild(name):GetChild('Judgment')
            rec_print_children(judgment)
            judgments[i] = judgment:GetChild('JudgmentWithOffsets')
            judgments[i]:Load('finalization-a 2x6.png')
        end
        self:SetUpdateFunction(function()
            local next_phase = math.floor(GAMESTATE:GetCurMusicSeconds() * 8)
            if next_phase ~= phase then
                phase = next_phase
                local path = phase % 2 == 0 and 'finalization-a 2x6.png' or 'finalization-b 2x6.png'
                for _, sprite in ipairs(sprites) do sprite:Load(path) end
                for _, judgment in ipairs(judgments) do judgment:Load(path) end
                for i = 0, 7 do MESSAGEMAN:Broadcast('Pulse' .. i) end
            end
        end)
    end,
}
for i = 1, 32 do
    root[#root + 1] = Def.Sprite{
        Name = 'Sprite' .. i,
        Texture = 'finalization-a 2x6.png',
        InitCommand = function(self) sprites[i] = self self:xy(i, -i) end,
    }
end
mod_actions = {}
for i = 1, 64 do mod_actions[i] = {i / 64, 'Pulse' .. (i % 8), true} end
return root
