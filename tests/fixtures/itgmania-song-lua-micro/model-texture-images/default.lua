local piece = GAMESTATE:GetCurrentSong():GetSongDir() .. "model.txt"
return Def.ActorFrame {
  Name="Root", FOV=0,
  Def.Model {
    Name="Animated", Meshes=piece, Materials=piece, Bones=piece,
    InitCommand=function(self)
      self:xy(100,200):glow(1,0,0,0.25):SetTextureFiltering(false)
        :texturewrapping(true):sleep(0.5):queuecommand("Select")
    end,
    SelectCommand=function(self) self:setstate(1) end,
  },
}
