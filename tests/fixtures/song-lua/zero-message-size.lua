local duration = 1
local step = 0
return Def.ActorFrame{
    OnCommand=function(self)
        self:SetUpdateFunction(function()
            local beat=GAMESTATE:GetSongBeat()
            if step==0 and beat>=1 then
                duration=0; step=1; MESSAGEMAN:Broadcast("Close")
            elseif step==1 and beat>=2 then
                duration=0.7; step=2; MESSAGEMAN:Broadcast("Open")
            elseif step==2 and beat>=3 then
                step=3; MESSAGEMAN:Broadcast("Close")
            end
        end)
    end,
    Def.Sprite{
        Name="Door",
        Texture="Normal 2x6.png",
        OnCommand=function(self) self:x(427):y(240):zoomto(96,96):diffusealpha(0) end,
        CloseMessageCommand=function(self) self:linear(duration/2):zoomto(64,64):diffusealpha(1) end,
        OpenMessageCommand=function(self) self:sleep(duration*0.25):linear(duration*0.75):zoomto(96,96):diffusealpha(0) end,
    },
}
