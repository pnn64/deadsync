local sent = 1
local messages = {{14, "LightsOff"}, {15, "LightsOn"}, {16.25, "Reveal"}}
return Def.ActorFrame {
  OnCommand=function(self)
    self:sleep(1000)
    self:SetUpdateFunction(function()
      while sent <= #messages and GAMESTATE:GetSongBeat() >= messages[sent][1] do
        MESSAGEMAN:Broadcast(messages[sent][2])
        sent = sent + 1
      end
    end)
  end,
  Def.Quad {
    Name="Fade",
    InitCommand=function(self) self:FullScreen():diffusealpha(0) end,
    OnCommand=function(self) self:diffusealpha(0):linear(0.3):diffusealpha(1):sleep(4):linear(0.5):diffusealpha(0) end,
    LightsOffMessageCommand=function(self) self:diffusealpha(1):linear(0.75):diffusealpha(0) end,
    LightsOnMessageCommand=function(self) self:diffusealpha(0):linear(0.8):diffusealpha(0) end,
    RevealMessageCommand=function(self) self:diffusealpha(1) end,
  },
}
