local ps = GAMESTATE:GetPlayerState(PLAYER_1)
local song = ps:GetPlayerOptions('ModsLevel_Song')
local current = ps:GetCurrentPlayerOptions()
local stage = ps:GetPlayerOptions('ModsLevel_Stage')
local preferred = ps:GetPlayerOptions('ModsLevel_Preferred')
assert(current == ps:GetPlayerOptions('ModsLevel_Current'))
assert(current ~= song and stage ~= preferred)
assert(not pcall(function() ps:GetPlayerOptions('bad level') end))
assert(current:XMod() == 1 and current:CMod() == nil and current:MMod() == nil)
assert(GAMESTATE:GetSongOptionsObject('ModsLevel_Current'):SaveScore() == true)
local phase = 0
return Def.ActorFrame{OnCommand=function(self)
    song:Dark(0.5, 2)
    assert(song:Dark() == 0.5 and current:Dark() == 0)
    assert(not GAMESTATE:PlayerIsUsingModifier(PLAYER_1, '50% dark'))
    self:SetUpdateFunction(function()
        local t = GAMESTATE:GetCurMusicSeconds()
        if phase == 0 then
            assert(math.abs(current:Dark() - math.min(0.5, t * 2)) < 0.000001)
            -- At exactly .25 repeated native float steps can remain just below
            -- .5; the following frame reaches the target without a tolerance.
            if t > 0.25 then
                assert(current:Dark() == 0.5)
                assert(GAMESTATE:PlayerIsUsingModifier(PLAYER_1, '50% dark'))
                song:Dark(0, 0)
                phase = 1
            end
        elseif phase == 1 and t >= 0.5 then
            assert(current:Dark() == 0.5, 'Current must not alias a zero-speed Song target')
            assert(ps:SetPlayerOptions('ModsLevel_Stage', '25% dark') == nil)
            assert(current:Dark() == 0.25 and song:Dark() == 0.25 and stage:Dark() == 0.25)
            assert(preferred:Dark() == 0)
            preferred:Dark(0.75)
            assert(song:Dark() == 0.25, 'direct Preferred writes do not propagate')
            ps:SetPlayerOptions('ModsLevel_Preferred', '50% dark')
            assert(current:Dark() == 0.5 and song:Dark() == 0.5 and stage:Dark() == 0.5)
            phase = 2
        end
    end)
end}
