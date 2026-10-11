local piece = "model.txt"
local function model(name, x, init)
  return Def.Model {
    Name=name, Meshes=piece, Materials=piece, Bones=piece,
    InitCommand=function(self)
      self:xy(x,200):glow(1,0,0,0.25):setstate(1)
      init(self)
    end,
    SelectCommand=function(self) self:setstate(0) end,
  }
end
return Def.ActorFrame {
  Name="Root", FOV=0,
  model("Queued",100,function(self) self:sleep(1):queuecommand("Select") end),
  model("Sleeping",200,function(self) self:hibernate(0.5) end),
  Def.ActorFrame {
    Name="SleepingParent",
    InitCommand=function(self) self:hibernate(0.5) end,
    model("SleepingChild",300,function(self) end),
  },
  Def.ActorFrame {
    Name="FastParent",
    InitCommand=function(self) self:SetUpdateRate(2) end,
    model("FastChild",400,function(self) self:sleep(1):queuecommand("Select") end),
  },
}
