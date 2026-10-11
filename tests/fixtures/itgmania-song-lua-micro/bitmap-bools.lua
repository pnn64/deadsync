return Def.ActorFrame{
 OnCommand=function(self)
  local native_label=self:GetChild("Label")
local label=native_label
for _, name in ipairs({"rainbowscroll", "jitter", "uppercase"}) do
 assert(label[name](label, true)==label and label[name](label, false)==label, name.." accepts booleans and returns self")
 assert(not pcall(label[name], label), name.." rejects a missing boolean")
 assert(not pcall(label[name], label, nil), name.." rejects nil")
 for _, value in ipairs({0, 1, "false", {}, function() end}) do
  assert(not pcall(label[name], label, value), name.." requires a boolean")
 end
end
 end,
 Def.BitmapText{Name="Label",Font="Common Normal",Text=""},
}
