return Def.ActorFrame{
 OnCommand=function(self)
 local native_target=self:GetChild("Target")
 local native_proxy=self:GetChild("Proxy")
 local native_label=self:GetChild("Label")
 local frozen=self:GetChild("Frozen")
local actors={{native_target,"Actor"},{native_proxy,"ActorProxy"},{native_label,"BitmapText"},{frozen,"ActorFrameTexture"}}
local seen={}
for _, pair in ipairs(actors) do
 local actor, kind=pair[1],pair[2]
 local before=tostring(actor)
 assert(before:match("^"..kind.." %([%da-fA-Fx]+%)$"), "native type and opaque pointer format")
 assert(not seen[before], "different objects have different identities")
 seen[before]=true
 local name=actor:GetName()
 actor:name("PlayerP1/ActorFrame/NativeNameTrap")
 assert(tostring(actor)==before, "renaming cannot change actor identity")
 assert(not tostring(actor):find("NativeNameTrap",1,true), "name and hierarchy stay out of tostring")
 actor:name(name)
end
 end,
 Def.Actor{Name="Target"},
 Def.ActorProxy{Name="Proxy"},
 Def.BitmapText{Name="Label",Font="Common Normal",Text=""},
 Def.ActorFrameTexture{Name="Frozen"},
}
