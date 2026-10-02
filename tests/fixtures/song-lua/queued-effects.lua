local fired=false
return Def.ActorFrame{
    Def.Quad{Name="Driver",InitCommand=cmd(queuecommand,"Update"),OnCommand=cmd(visible,false),UpdateCommand=function(self)
        if not fired and GAMESTATE:GetSongBeat()>=0.19 then fired=true;MESSAGEMAN:Broadcast("Fire") end
        self:sleep(0.02):queuecommand("Update")
    end},
    Def.ActorFrame{Name="Cala",FireMessageCommand=cmd(playcommand,"StartFire";sleep,0.5;queuecommand,"Main"),MainCommand=cmd(vibrate;effectmagnitude,10,10,0;sleep,0.3;queuecommand,"Done"),DoneCommand=cmd(stopeffect;queuecommand,"FinishFire"),
        Def.Quad{Name="Idle",OnCommand=cmd(zoomto,64,32;xy,100,100),StartFireCommand=cmd(visible,false),FinishFireCommand=cmd(sleep,0.5;queuecommand,"Show"),ShowCommand=cmd(visible,true)},
        Def.Quad{Name="Fire",OnCommand=cmd(zoomto,64,32;xy,100,200;visible,false),StartFireCommand=cmd(visible,true),FinishFireCommand=cmd(visible,false)},
    },
}
