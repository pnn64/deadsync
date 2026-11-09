-- Allocated native actors are userdata. Their independent handles use tables,
-- so this debug walk must not traverse the owned actor's internal cycles.
local function rec_print_type(value)
    local kind = type(value)
    if kind == "table" and (rawget(value, "__songlua_def_owned") == true
        or type(rawget(value, "__songlua_actor_type")) == "string") then
        return "userdata"
    end
    return kind
end
function rec_print_table(t, indent, depth_remaining)
    if not indent then indent = "" end
    if rec_print_type(t) ~= "table" then
        Trace(indent .. "rec_print_table passed a " .. rec_print_type(t))
        return
    end
    depth_remaining = depth_remaining or -1
    if depth_remaining == 0 then return end
    for k, v in pairs(t) do
        if rec_print_type(v) == "table" then
            Trace(indent .. k .. ": table")
            rec_print_table(v, indent .. "  ", depth_remaining - 1)
        else
            Trace(indent .. "(" .. rec_print_type(k) .. ")" .. k .. ": " ..
                "(" .. rec_print_type(v) .. ")" .. tostring(v))
        end
    end
    Trace(indent .. "end")
end
function rec_print_children(parent, indent)
    indent = indent or ""
    if #parent > 0 and type(parent) == "table" then
        for i, child in ipairs(parent) do rec_print_children(child, indent .. i .. "->") end
    elseif parent.GetChildren then
        local name = (parent.GetName and parent:GetName()) or ""
        Trace(indent .. name .. " children:")
        for key, child in pairs(parent:GetChildren()) do
            if #child > 0 then
                Trace(indent .. name .. "->" .. key .. " shared name:")
                rec_print_children(child, indent .. name .. "->")
                Trace(indent .. name .. "->" .. key .. " shared name over.")
            else rec_print_children(child, indent .. name .. "->") end
        end
        Trace(indent .. name .. " children over.")
    else
        local name = (parent.GetName and parent:GetName()) or ""
        Trace(indent .. name .. "(" .. tostring(parent) .. ")")
    end
end
