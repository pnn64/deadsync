
        bg_fit_functions = {
            BackgroundFitMode_CoverDistort = function(self, width, height)
                self:zoomx(width / self:GetWidth())
                self:zoomy(height / self:GetHeight())
            end,
            BackgroundFitMode_CoverPreserve = function(self, width, height)
                self:zoom(math.max(width / self:GetWidth(), height / self:GetHeight()))
            end,
            BackgroundFitMode_FitInside = function(self, width, height)
                self:zoom(math.min(width / self:GetWidth(), height / self:GetHeight()))
            end,
            BackgroundFitMode_FitInsideAvoidLetter = function(self, width, height)
                self:zoom(height / self:GetHeight())
            end,
            BackgroundFitMode_FitInsideAvoidPillar = function(self, width, height)
                self:zoom(width / self:GetWidth())
            end,
        }
        -- _fallback/Scripts/02 Utilities.lua: use the current style type and
        -- preference rather than the number of enabled players.
        function Center1Player()
            local styleType = GAMESTATE:GetCurrentStyle():GetStyleType()
            if styleType == "StyleType_OnePlayerTwoSides"
                or styleType == "StyleType_TwoPlayersSharedSides" then
                return true
            elseif PREFSMAN:GetPreference("Center1Player") then
                return styleType == "StyleType_OnePlayerOneSide"
            else
                return false
            end
        end
    