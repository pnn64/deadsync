return Def.ActorFrame {
    OnCommand = cmd(SetUpdateFunction, function(self)
        local count = 0;
        for i = 1, 3 do
            if i == 1 then count = count + i;
            elseif i == 2 then do count = count + i; end
            else count = count + i; end
        end
        while count < 7 do count = count + 1; end
        repeat count = count + 1; until count == 8
        local nested = function() return "function; do; end", count; end;
        local label, result = nested();
        SCREENMAN:GetTopScreen():GetChild("PlayerP1"):x(241 + result);
    end; y, 12)
}
