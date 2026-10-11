return Def.ActorFrame{
 OnCommand=function(self)
 local native_sprite=self:GetChild("Image")
 local fixture_path=GAMESTATE:GetCurrentSong():GetSongDir().."fit-rect.png"
for _, name in ipairs({"LoadBackground", "LoadBanner"}) do
 assert(native_sprite[name](native_sprite,fixture_path)==fixture_path, name.." returns the top argument, not self")
 assert(native_sprite:GetWidth()==64 and native_sprite:GetHeight()==32, name.." resets source frame size")
 local texture=native_sprite:GetTexture()
 assert(texture and texture:GetSourceWidth()==64 and texture:GetSourceHeight()==32, name.." installs a texture")
 assert(native_sprite[name](native_sprite,fixture_path,42)==42, name.." returns the last stack argument")
 assert(native_sprite[name](native_sprite,fixture_path,nil)==nil, name.." retains a trailing nil return")
 assert(not pcall(native_sprite[name],native_sprite), name.." requires a path")
 assert(not pcall(native_sprite[name],native_sprite,false), name.." requires a string")
end
 end,
 Def.Sprite{Name="Image"},
}
