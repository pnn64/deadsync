local fader, started
return Def.ActorFrame{
    Def.Actor{
        OnCommand=function(self) self:queuecommand('Loop') end,
        LoopCommand=function(self)
            if not started and GAMESTATE:GetSongBeat() >= 412 then
                fader:linear(2):diffusealpha(1)
                started = true
            end
            self:sleep(1/60):queuecommand('Loop')
        end,
    },
    Def.Quad{
        Name='Fade',
        InitCommand=function(self) fader = self end,
        OnCommand=function(self) self:diffusealpha(0) end,
    },
}
