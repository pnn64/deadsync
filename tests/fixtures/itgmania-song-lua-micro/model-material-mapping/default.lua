local dir = GAMESTATE:GetCurrentSong():GetSongDir()
return Def.ActorFrame {
  Name="Root", FOV=0,
  Def.Model {
    Name="DiffuseSphere", Meshes=dir.."diffuse.txt", Materials=dir.."diffuse.txt", Bones=dir.."diffuse.txt",
    InitCommand=function(self) self:xy(100,200):glow(1,0,0,0.25):SetTextureFiltering(false):texturewrapping(true) end,
  },
  Def.Model {
    Name="SecondarySphere", Meshes=dir.."secondary.txt", Materials=dir.."secondary.txt", Bones=dir.."secondary.txt",
    InitCommand=function(self) self:xy(150,200):glow(1,0,0,0.25):SetTextureFiltering(false):texturewrapping(true) end,
  },
}
