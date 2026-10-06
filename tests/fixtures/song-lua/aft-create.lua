local frozen, collision, unused, output, late, late_output
local errors = 0
local function texture_shape(texture, name, width, height, backing_width, backing_height)
    assert(texture ~= nil, "created texture must exist")
    assert(texture:GetPath() == name, "texture path is its allocated name")
    assert(texture:GetSourceWidth() == width and texture:GetSourceHeight() == height,
        "source dimensions are frozen integer dimensions")
    assert(texture:GetImageWidth() == width and texture:GetImageHeight() == height,
        "image dimensions are frozen integer dimensions")
    assert(texture:GetTextureWidth() == backing_width and texture:GetTextureHeight() == backing_height,
        "backing dimensions are powers of two")
end

return Def.ActorFrame{
    OnCommand=function(self)
        assert(frozen:GetTexture() == nil and unused:GetTexture() == nil,
            "uncreated captures return nil")
        for _, method in ipairs({"EnableAlphaBuffer", "EnableDepthBuffer", "EnableFloat", "EnablePreserveTexture"}) do
            assert(not pcall(function() frozen[method](frozen) end), method .. " requires a boolean")
            for _, value in ipairs({0, 1, "true", {}}) do
                assert(not pcall(function() frozen[method](frozen, value) end),
                    method .. " rejects truthy non-booleans")
            end
        end
        assert(not pcall(function() frozen:SetTextureName(false) end), "name requires a string")
        frozen:setsize(0.75, 1)
        assert(pcall(function() assert(frozen:Create() == frozen) end), "invalid Create returns self")
        assert(frozen:GetTexture() == nil and errors == 1, "invalid size reports an error without allocating")
        frozen:setsize(65.75, 33.5):SetTextureName("temp/../FrozenCapture")
            :EnableAlphaBuffer(true):EnableDepthBuffer(true):EnableFloat(true):Create()
        local retained = frozen:GetTexture()
        assert(frozen:GetTexture() == retained, "retained handles keep texture identity")
        texture_shape(retained, "FrozenCapture", 65, 33, 128, 64)
        frozen:setsize(17, 9):SetTextureName("RequestedName")
            :EnableAlphaBuffer(false):EnableDepthBuffer(false):EnableFloat(false)
        texture_shape(frozen:GetTexture(), "FrozenCapture", 65, 33, 128, 64)
        texture_shape(retained, "FrozenCapture", 65, 33, 128, 64)
        assert(pcall(function() assert(frozen:Create() == frozen) end), "repeated Create returns self")
        assert(errors == 2, "repeated Create reports an error")
        texture_shape(frozen:GetTexture(), "FrozenCapture", 65, 33, 128, 64)
        collision:setsize(32, 16):SetTextureName("FrozenCapture"):Create()
        assert(collision:GetTexture() == nil and errors == 3, "registered names cannot allocate twice")
        collision:SetTextureName("CollisionRetry"):Create()
        texture_shape(collision:GetTexture(), "CollisionRetry", 32, 16, 32, 16)
        output:SetTexture(retained)
        self:SetUpdateFunction(function()
            local beat = GAMESTATE:GetSongBeat()
            if beat >= 1 and late:GetTexture() == nil then
                late:setsize(23.9, 11.9):SetTextureName("LateCapture"):Create()
                late_output:SetTexture(late:GetTexture())
            end
            frozen:EnablePreserveTexture(beat >= 1)
            if beat >= 2 then
                texture_shape(late:GetTexture(), "LateCapture", 23, 11, 32, 16)
            end
        end)
    end,
    ScriptErrorMessageCommand=function(self, params)
        assert(type(params.message) == "string")
        errors = errors + 1
        self:x(errors)
        lua.ReportScriptError("nested reporting must not recurse")
    end,
    Def.ActorFrameTexture{
        Name="Frozen",
        InitCommand=function(self) frozen = self end,
        Def.Quad{ InitCommand=function(self) self:zoomto(4, 4) end },
    },
    Def.ActorFrameTexture{
        Name="Collision", InitCommand=function(self) collision = self end,
    },
    Def.ActorFrameTexture{
        Name="Unused", InitCommand=function(self) unused = self end,
        Def.Quad{ Name="SuppressedChild" },
    },
    Def.Sprite{ Name="Output", InitCommand=function(self) output = self end },
    Def.ActorFrameTexture{
        Name="Late", InitCommand=function(self) late = self end,
        Def.Quad{ InitCommand=function(self) self:zoomto(4, 4) end },
    },
    Def.Sprite{ Name="LateOutput", InitCommand=function(self) late_output = self end },
}
