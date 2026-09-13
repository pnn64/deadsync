local width = PREFSMAN:GetPreference("DisplayWidth")
local height = PREFSMAN:GetPreference("DisplayHeight")
local root = Def.ActorFrame{}
for index, name in ipairs({"ScreenTex", "VStripsTex", "HStripsTex"}) do
    local capture = Def.ActorFrameTexture{
        Name = name,
        InitCommand = function(self)
            self:SetTextureName(name):SetWidth(width):SetHeight(height):Create()
        end,
    }
    if index == 1 then
        capture[1] = Def.Quad{
            InitCommand = function(self)
                self:xy(width / 4, height / 2):zoomto(width / 2, height):diffuse(1, 0, 0, 1)
            end,
        }
        capture[2] = Def.Quad{
            InitCommand = function(self)
                self:xy(width * 3 / 4, height / 2):zoomto(width / 2, height):diffuse(0, 1, 0, 1)
            end,
        }
    else
        capture[1] = Def.Sprite{
            Texture = index == 2 and "ScreenTex" or "VStripsTex",
            InitCommand = function(self) self:xy(width / 2, height / 2) end,
        }
    end
    root[#root + 1] = capture
end
root[#root + 1] = Def.Sprite{
    Name = "Output",
    Texture = "HStripsTex",
    InitCommand = function(self)
        self:xy(SCREEN_CENTER_X, SCREEN_CENTER_Y)
            :zoomx(SCREEN_WIDTH / width):zoomy(SCREEN_HEIGHT / height)
    end,
}
return root
