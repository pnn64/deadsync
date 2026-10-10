return Def.ActorFrame{
    Def.Model{Meshes="model.txt", Materials="model.txt", Bones="model.txt", Name="Zero", InitCommand=function(self) self:xy(400,240):diffusealpha(0):glow{1,0.2,0.4,0} end},
    Def.Model{Meshes="model.txt", Materials="model.txt", Bones="model.txt", Name="Below", InitCommand=function(self) self:xy(400,240):diffusealpha(0.0009999):glow{1,0.2,0.4,0} end},
    Def.Model{Meshes="model.txt", Materials="model.txt", Bones="model.txt", Name="Equal", InitCommand=function(self) self:xy(400,240):diffusealpha(0.001):glow{1,0.2,0.4,0} end},
    Def.Model{Meshes="model.txt", Materials="model.txt", Bones="model.txt", Name="Above", InitCommand=function(self) self:xy(400,240):diffusealpha(0.0010001):glow{1,0.2,0.4,0} end},
    Def.Model{Meshes="model.txt", Materials="model.txt", Bones="model.txt", Name="BothBelow", InitCommand=function(self) self:xy(400,240):diffusealpha(0.0005):glow{1,0.2,0.4,0.0005} end},
    Def.Model{Meshes="model.txt", Materials="model.txt", Bones="model.txt", Name="GlowBelow", InitCommand=function(self) self:xy(400,240):diffusealpha(0):glow{1,0.2,0.4,0.0009999} end},
    Def.Model{Meshes="model.txt", Materials="model.txt", Bones="model.txt", Name="GlowEqual", InitCommand=function(self) self:xy(400,240):diffusealpha(0):glow{1,0.2,0.4,0.001} end},
    Def.Model{Meshes="model.txt", Materials="model.txt", Bones="model.txt", Name="GlowAbove", InitCommand=function(self) self:xy(400,240):diffusealpha(0):glow{1,0.2,0.4,0.0010001} end},
    Def.Model{Meshes="model.txt", Materials="model.txt", Bones="model.txt", Name="DiffuseBelowGlowEqual", InitCommand=function(self) self:xy(400,240):diffusealpha(0.0005):glow{1,0.2,0.4,0.001} end},
    Def.Model{Meshes="model.txt", Materials="model.txt", Bones="model.txt", Name="DiffuseEqualGlowBelow", InitCommand=function(self) self:xy(400,240):diffusealpha(0.001):glow{1,0.2,0.4,0.0005} end},
    Def.Model{Meshes="model.txt", Materials="model.txt", Bones="model.txt", Name="TinyDiffuseGlowEqual", InitCommand=function(self) self:xy(400,240):diffusealpha(5e-07):glow{1,0.2,0.4,0.001} end},
}
