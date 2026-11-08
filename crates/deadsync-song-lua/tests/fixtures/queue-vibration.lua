mod_actions = {{1, "CalaFire", true}}
local fired = false
return Def.ActorFrame {
    OnCommand = function(self)
        self:SetUpdateFunction(function()
            if not fired and GAMESTATE:GetSongBeat() >= 1 then
                fired = true
                MESSAGEMAN:Broadcast("CalaFire")
            end
        end)
    end,
    Def.ActorFrame {
        Name = "Cala",
        CalaFireMessageCommand = cmd(playcommand,"Fire";sleep,0.5;queuecommand,"Main"),
        MainCommand = cmd(vibrate;effectmagnitude,10,10,0;sleep,0.3;queuecommand,"Done"),
        DoneCommand = cmd(stopeffect;queuecommand,"FinishFire"),
        Def.Quad {
            Name = "Body",
            OnCommand = cmd(zoomto,64,32),
            FireCommand = cmd(visible,false),
            FinishFireCommand = cmd(visible,true),
        },
        Def.Quad { Name = "EffectWitness", OnCommand = cmd(x,100;zoomto,64,32) },
    },
}
