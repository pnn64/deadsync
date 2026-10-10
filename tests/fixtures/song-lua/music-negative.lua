return Def.ActorFrame {
    Name='MusicSeconds',
    OnCommand=function(self)
        self:SetUpdateFunction(function(self)
            local p=GAMESTATE:GetSongPosition()
            local values={p:GetMusicSeconds(), p:GetMusicSecondsVisible(), GAMESTATE:GetCurMusicSeconds(),
                GAMESTATE:GetPlayerState(PLAYER_1):GetSongPosition():GetMusicSeconds(),
                GAMESTATE:GetPlayerState(PLAYER_2):GetSongPosition():GetMusicSecondsVisible(), GetTimeSinceStart()}
            for i, value in ipairs(values) do self:GetChild('Clock'..i):x(value*10) end
        end)
    end,
    Def.Quad {Name='Clock1', InitCommand=function(self) self:setsize(2,2) end},
    Def.Quad {Name='Clock2', InitCommand=function(self) self:setsize(2,2) end},
    Def.Quad {Name='Clock3', InitCommand=function(self) self:setsize(2,2) end},
    Def.Quad {Name='Clock4', InitCommand=function(self) self:setsize(2,2) end},
    Def.Quad {Name='Clock5', InitCommand=function(self) self:setsize(2,2) end},
    Def.Quad {Name='Clock6', InitCommand=function(self) self:setsize(2,2) end},
}
