local graph,loaded
return Def.ActorFrame {
 OnCommand=function()
  local image=assert(loaded:GetTexture())
  local before=image:GetPath()
  graph:SetTexture(image)
  local copied=assert(graph:GetTexture())
  local message=string.format('graph=%s image=%s rawimage=%s tag=%s retained=%s rawgraph=%s kind=%s',
   copied:GetPath(),before,tostring(rawget(image,'__songlua_texture_path')),
   tostring(rawget(image,'__songlua_native_type')),tostring(rawequal(rawget(graph,'__songlua_amv_texture_handle'),image)),
   tostring(rawget(graph,'Texture')),tostring(rawget(graph,'__songlua_actor_type')))
  print(message)
  assert(copied:GetPath()==before,message)
 end,
 Def.ActorMultiVertex {InitCommand=function(self)graph=self end},
 Def.Sprite {Texture='__songlua_theme_path/G/_Judgments/Love 2x6.png',InitCommand=function(self)loaded=self end},
}
