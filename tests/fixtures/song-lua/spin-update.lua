local beat, speed, faded, turned, stopped, resumed, context, finished
return Def.ActorFrame{
    OnCommand=function(self)
        self:SetUpdateFunction(function()
            local current=GAMESTATE:GetSongBeat()
            if current<=0 then return end
            beat=current
            if beat>=0.2 and not speed then speed=true;MESSAGEMAN:Broadcast("Speed") end
            if beat>=0.4 and not faded then faded=true;MESSAGEMAN:Broadcast("Fade") end
            if beat>=0.7 and not turned then turned=true;MESSAGEMAN:Broadcast("Turn") end
            if beat>=1.1 and not stopped then stopped=true;MESSAGEMAN:Broadcast("Stop") end
            if beat>=1.4 and not resumed then resumed=true;MESSAGEMAN:Broadcast("Resume") end
            if beat>=1.8 and not context then context=true;MESSAGEMAN:Broadcast("Context") end
            if beat>=2.2 and not finished then finished=true;MESSAGEMAN:Broadcast("Finish") end
        end)
    end,
    Def.Quad{
        Name="Spinning",
        OnCommand=cmd(zoomto,64,32;xy,100,100;rotationz,350;spin;effectmagnitude,0,0,90),
        SpeedMessageCommand=cmd(spin;effectmagnitude,0,0,-30),
        FadeMessageCommand=cmd(linear,0.5;diffusealpha,0.5),
        TurnMessageCommand=cmd(rotationz,45),
        StopMessageCommand=cmd(stoptweening;stopeffect),
        ResumeMessageCommand=cmd(spin;effectmagnitude,0,0,45),
        ContextMessageCommand=function(self) self:zoom(0.25+math.sin(beat)*2):diffusealpha(0.2):sleep(0.0266666) end,
        FinishMessageCommand=cmd(finishtweening),
    },
    Def.Quad{
        Name="TweenSpin",
        OnCommand=cmd(zoomto,64,32;xy,100,200;rotationz,10;spin;effectmagnitude,0,0,60),
        FadeMessageCommand=cmd(linear,0.4;diffusealpha,0.2),
    },
    Def.Quad{
        Name="BeatSpin",
        OnCommand=cmd(zoomto,64,32;xy,250,100;rotationz,20;spin;effectclock,"beat";effectmagnitude,0,0,10),
    },
    Def.Quad{
        Name="StaticReceiver",OnCommand=cmd(zoomto,32,16;xy,250,200),
        ContextMessageCommand=cmd(diffusealpha,0.8),
    },
}
