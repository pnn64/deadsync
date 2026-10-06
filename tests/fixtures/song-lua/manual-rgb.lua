local vertices = {
    {{0,0,0}, {1,1,1,1}, {0,0}},
    {{0,10,0}, {1,1,1,1}, {0,1}},
    {{10,0,0}, {1,1,1,1}, {1,0}},
    {{10,10,0}, {1,1,1,1}, {1,1}},
}
return Def.ActorFrame {
    Name="DrawRoot",
    InitCommand=function(self) self:xy(40,50):SetFOV(45) end,
    OnCommand=function(self)
        local target, mesh = self:GetChild("target"), self:GetChild("mesh")
        target:SetSize(640,480):EnableAlphaBuffer(true):EnableDepthBuffer(true)
            :EnableFloat(true):EnablePreserveTexture(true):Create()
        local texture = target:GetTexture()
        mesh:SetTexture(texture):SetVertices(vertices)
            :SetDrawState{Mode="DrawMode_QuadStrip", First=1, Num=-1}
        self:SetDrawFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            texture:BeginRenderingTo(beat >= 1)
            mesh:xy(beat,0):diffuse(1,0,0,1):Draw()
            mesh:xy(beat+20,0):diffuse(0,1,0,1):Draw()
            vertices[1][1][2] = beat
            mesh:SetVertices(vertices):xy(beat+40,0):diffuse(0,0,1,1):Draw()
            texture:FinishRenderingTo()
        end)
    end,
    Def.ActorFrameTexture{Name="target"},
    Def.ActorMultiVertex{Name="mesh"},
}
