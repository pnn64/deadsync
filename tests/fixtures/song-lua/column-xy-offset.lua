local probes = {}
local children = {Name="ColumnXYControl"}
for player=1,2 do
    probes[player] = {}
    for _,column in ipairs{1,4} do
        children[#children+1] = Def.Actor{Name="P"..player.."C"..column,
            InitCommand=function(self) probes[player][column] = self end}
    end
end
local queries = {}
for index=1,14 do
    children[#children+1] = Def.Actor{Name="Query"..index,
        InitCommand=function(self) queries[index] = self end}
end
local queried = false
local previous = 0
children.InitCommand=function(self)
    self:SetUpdateFunction(function(self)
        if not queried then
            queried = true
            local spline = SCREENMAN:GetTopScreen():GetChild("PlayerP1"):GetChild("NoteField"):GetColumnActors()[2]:GetPosHandler():GetSpline()
            local function record(index,t)
                local point = spline:evaluate(t)
                queries[index]:x(point[1]):y(point[2]):z(point[3])
            end
            record(1,0.5) -- An empty native spline evaluates to zero.
            spline:SetSize(2):SetPoint(1,{2,-3,4}):SetPoint(2,{8,9,-2})
            record(2,0.5) -- SetPoint does not solve coefficients implicitly.
            spline:Solve()
            record(3,0.5)
            spline:SetPoint(1,{4,-1,2})
            record(4,0.5) -- Preserve solved slopes until the next Solve.
            spline:SetSize(4):SetPoint(1,{1,-2,3}):SetPoint(2,{5,4,-1})
                :SetPoint(3,{-3,9,7}):SetPoint(4,{8,-6,2}):Solve()
            for index,t in ipairs{-2,-0.5,0,0.25,0.5,1,1.5,2.75,3,7} do
                record(index+4,t)
            end
        end
        local beat = GAMESTATE:GetSongBeat()
        local phase = beat >= 0.5 and 2 or beat >= 0.25 and 1 or 0
        if phase == 0 or phase == previous then return end
        previous = phase
        for player=1,2 do
            local columns = SCREENMAN:GetTopScreen():GetChild("PlayerP"..player):GetChild("NoteField"):GetColumnActors()
            for _,column in ipairs{1,4} do
                local handler = columns[column]:GetPosHandler()
                handler:SetSplineMode("NoteColumnSplineMode_Offset")
                handler:SetBeatsPerT(1)
                local spline = handler:GetSpline()
                spline:SetSize(1):SetPoint(1,{(player*3+column/4)*phase,(-player*4+column/5)*phase,0}):Solve()
                -- The linked LunaCubicSplineN evaluates this native spline.
                local point = spline:evaluate(-1)
                local other = spline:evaluate(7)
                assert(point[1] == other[1] and point[2] == other[2])
                probes[player][column]:x(point[1]):y(point[2])
            end
        end
    end)
end
return Def.ActorFrame(children)
