local poses = {
    {150,230,35, -12,17,20, 1.2,.8,.6, .1,-.05},
    {420,250,-40, 10,-24,-15, .7,1.3,0, -.07,.09},
    {280,210,10, 0,0,0, 0,0,0, 0,0},
}
return Def.ActorFrame {
    Name="caller",
    InitCommand=function(self) self:xy(11,17):rotationz(7):zoomx(1.1):zoomy(.9) end,
    OnCommand=function(self)
        local screen = SCREENMAN:GetTopScreen()
        screen:xy(31,-9)
        screen:GetChild("Overlay"):visible(false)
        local player = screen:GetChild("PlayerP1")
        player:AddWrapperState()
        player:AddWrapperState()
        player:GetWrapperState(1):xy(-4,6):z(-9):zoom(1.05)
        player:GetWrapperState(2):xy(7,11):z(12):rotationz(3)
        player:fov(55):vanishpoint(310,200)
        player:xy(320,240):zoom(0):diffusealpha(0):visible(false)
        local target = self:GetChild("capture")
        target:setsize(640,480):EnableAlphaBuffer(true):EnableDepthBuffer(true):Create()
        local texture = target:GetTexture()
        local function draw_players()
            for _, pose in ipairs(poses) do
                player:visible(true):diffuse(.2,.5,.8,1)
                    :xy(pose[1],pose[2]):z(pose[3])
                    :rotationx(pose[4]):rotationy(pose[5]):rotationz(pose[6])
                    :zoomx(pose[7]):zoomy(pose[8]):zoomz(pose[9])
                    :skewx(pose[10]):skewy(pose[11]):Draw()
            end
            -- The prepared final player cannot recover any of these draws.
            player:xy(320,240):zoom(0):diffusealpha(0):visible(false)
        end
        self:SetDrawFunction(function()
            texture:BeginRenderingTo(false)
            screen:GetChild("Underlay"):Draw()
            screen:GetChild("In"):Draw()
            draw_players()
            texture:FinishRenderingTo()
            screen:GetChild("Underlay"):Draw()
            screen:GetChild("In"):Draw()
            draw_players()
        end)
    end,
    Def.ActorFrameTexture{Name="capture"},
}
