
local quad, sprite, graph, target, loaded
return Def.ActorFrame {
    OnCommand=function()
        local texture = assert(quad:GetTexture())
        assert(texture:GetTextureWidth() == 1 and texture:GetTextureHeight() == 1)
        assert(sprite:GetTexture() == nil)
        assert(target:GetTexture() == nil)
        local initial = assert(graph:GetTexture(), 'AMV must load its native default texture')
        assert(initial:GetPath() == '' and initial:GetNumFrames() == 1)
        assert(initial:GetTextureWidth() == 1 and initial:GetTextureHeight() == 1)
        local image = assert(loaded:GetTexture())
        graph:SetTexture(image)
        local graph_texture = assert(graph:GetTexture(), 'AMV texture getter must still execute')
        assert(graph_texture:GetPath() == image:GetPath())
        assert(graph_texture:GetTextureWidth() == image:GetTextureWidth())
        assert(graph_texture:GetTextureHeight() == image:GetTextureHeight())
        graph:SetTexture(texture)
        local restored = assert(graph:GetTexture())
        assert(restored:GetPath() == '' and restored:GetNumFrames() == 1)
        assert(restored:GetTextureWidth() == 1 and restored:GetTextureHeight() == 1)
    end,
    Def.Quad {InitCommand=function(self) quad=self end},
    Def.Sprite {InitCommand=function(self) sprite=self end},
    Def.ActorMultiVertex {InitCommand=function(self) graph=self end},
    Def.ActorFrameTexture {InitCommand=function(self) target=self end},
    Def.Sprite {Texture='__songlua_theme_path/G/_Judgments/Love 2x6.png',
        InitCommand=function(self) loaded=self end},
}
