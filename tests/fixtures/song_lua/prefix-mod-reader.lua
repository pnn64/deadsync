prefix_globals = {ease = {{0, 1, 0, 5, 'centered', 'len', function(t,b,c,d)
    return b + c * t / d
end}}}

return Def.ActorFrame{
    OnCommand=function(self)
        local options = GAMESTATE:GetPlayerState(PLAYER_1):GetPlayerOptions('ModsLevel_Song')
        self:SetUpdateFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            for _, ease in ipairs(prefix_globals.ease) do
                if beat <= ease[1] + ease[2] then
                    local strength = ease[7](beat - ease[1], ease[3], ease[4] - ease[3], ease[2])
                    local number = strength .. ''
                    -- Legacy readers validate by removing a possible '%' suffix.
                    -- Lua 5.1 formats the peak as '5', so this deliberately writes 0.
                    local value = tonumber(number:sub(1, -2)) and tonumber(number) / 100 or 0
                    options:Centered(value, 9999)
                end
            end
        end)
    end,
}
