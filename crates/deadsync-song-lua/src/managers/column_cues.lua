return function(steps, pn)
    local cues = steps:GetColumnCues(SL.Global.ColumnCueMinTime)
    for _, cue in ipairs(cues) do
        for _, column in ipairs(cue.columns) do
            column.isMine = column.noteType == 4
            column.noteType = nil
        end
    end
    SL[pn].Streams.ColumnCues = cues
end
