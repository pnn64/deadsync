-- Private structural helpers for the Rust actor host. Instance state and command
-- execution live in actors.rs. Capture primitives before authored scripts run.
local pairs, next, rawget = pairs, next, rawget
local tonumber, tostring, type = tonumber, tostring, type
local setmetatable, getmetatable, ipairs, error = setmetatable, getmetatable, ipairs, error
local globals, getinfo, sub, match = _G, debug.getinfo, string.sub, string.match

return function(methods)
    Actor = methods.Actor
function Actor:drop(t)
    self:tween(t, "TweenType_Bezier", {0, 0, 1/3, 1, 2/3, 0.5, 1, 1})
    return self
end
function Actor:hidden(bHide)
    lua.ReportScriptError("hidden is deprecated, use visible instead. (used on " .. self:GetName() .. ")")
    self:visible(not bHide)
end
function Actor:LyricCommand(side)
	self:settext( Var "LyricText" )
	self:draworder(102)

	self:stoptweening()
	self:shadowlengthx(0)
	self:shadowlengthy(5)
	self:strokecolor(color("#000000"))

	local Zoom = SCREEN_WIDTH / (self:GetZoomedWidth()+1)
	if( Zoom > 1 ) then
		Zoom = 1
	end
	self:zoomx( Zoom )

	local lyricColor = Var "LyricColor"
	local Factor = 1
	if side == "Back" then
		Factor = 0.5
	elseif side == "Front" then
		Factor = 0.9
	end
	self:diffuse( {
		lyricColor[1] * Factor,
		lyricColor[2] * Factor,
		lyricColor[3] * Factor,
		lyricColor[4] * Factor } )

	if side == "Front" then
		self:cropright(1)
	else
		self:cropleft(0)
	end

	self:diffusealpha(0)
	self:linear(0.2)
	self:diffusealpha(0.75)
	self:linear( Var "LyricDuration" * 0.75)
	if side == "Front" then
		self:cropright(0)
	else
		self:cropleft(1)
	end
	self:sleep( Var "LyricDuration" * 0.25 )
	self:linear(0.2)
	self:diffusealpha(0)
	return self
end
    ActorFrame = setmetatable(methods.ActorFrame, {__index=Actor})
    ActorFrameTexture = setmetatable(methods.ActorFrameTexture, {__index=ActorFrame})
    BitmapText = setmetatable(methods.BitmapText, {__index=Actor})
    function BitmapText:PixelFont() self:SetTextureFiltering(false); return self end
    function BitmapText:Stroke(c) self:strokecolor(c); return self end
    function BitmapText:NoStroke() self:strokecolor(color("0,0,0,0")); return self end
    function BitmapText:settextf(...) self:settext(string.format(...)); return self end
    function BitmapText:DiffuseAndStroke(diffuseC,strokeC) self:diffuse(diffuseC); self:strokecolor(strokeC); return self end
    Sprite = setmetatable(methods.Sprite, {__index=Actor})

function Sprite.LinearFrames(NumFrames, Seconds)
    local Frames = {}
    for i = 0,NumFrames-1 do
        Frames[#Frames+1] = {Frame = i, Delay = (1/NumFrames)*Seconds}
    end
    return Frames
end
    Sprite.cropto = methods.CropToHelper
    Banner = setmetatable(methods.Banner, {__index=Sprite})
    ActorMultiVertex = setmetatable(methods.ActorMultiVertex, {__index=Actor})
    local function proxy_method(name)
        return function(actor, ...)
            if type(actor)~='table' then error('actor method '..name..' requires an actor') end
            local bound=rawget(actor,'__songlua_bound_methods')
            if bound~=nil and type(bound)~='table' then error('actor method table expected') end
            local method=rawget(bound or actor,name)
            if type(method)~='function' then error('native actor method '..name..' has no semantic implementation') end
            return method(actor, ...)
        end
    end
    -- The current-background Lua helper is stored on each actor. The native
    -- public forwarders still consult the private bound table, including
    -- authored replacements and the existing missing-implementation error.
    Sprite.LoadFromSongBanner = proxy_method('LoadFromSongBanner')
    Sprite.LoadFromCurrentSongBackground = proxy_method('LoadFromCurrentSongBackground')
    ActorProxy = setmetatable({SetTarget=proxy_method('SetTarget'), GetTarget=proxy_method('GetTarget')}, {__index=Actor})
    NoteField=setmetatable({}, {__index=ActorFrame})
    for _,name in ipairs{'set_step_callback','set_set_pressed_callback',
        'set_did_tap_note_callback','set_did_hold_note_callback','step','set_pressed',
        'did_tap_note','did_hold_note','get_column_actors','GetBeatBars','SetBeatBars','SetBeatBarsAlpha'} do
        NoteField[name]=proxy_method(name)
    end
    Player=setmetatable({}, {__index=ActorFrame})
    for _,name in ipairs{'SetLife','get_oitg_zoom_mode','set_oitg_zoom_mode'} do
        Player[name]=proxy_method(name)
    end
    local text_displays = {RollingNumbers=true, BPMDisplay=true, DeviceList=true, InputList=true}
    local private_frames = {TopScreen=true,StepsDisplay=true,PercentageDisplay=true,HoldJudgment=true,NoteField=true,PlayerActor=true,CourseContentsList=true,GraphDisplay=true, SongMeterDisplay=true}
    local private_actors = {OptionRow=true,OptionRowFrame=true,LoadActor=true,Timer=true,NoteColumnRenderer=true,LifeMeter=true,Model=true,GraphDisplayBody=true, GraphDisplayLine=true}
    local classes = {TopScreen=ActorFrame,OptionRow=Actor,OptionRowFrame=Actor,LoadActor=Actor,StepsDisplay=ActorFrame,PercentageDisplay=ActorFrame,Timer=Actor,HoldJudgment=ActorFrame,NoteField=NoteField,NoteColumnRenderer=Actor,PlayerActor=ActorFrame,LifeMeter=Actor,Model=Actor,CourseContentsList=ActorFrame,GraphDisplay=ActorFrame, SongMeterDisplay=ActorFrame, GraphDisplayBody=Actor, GraphDisplayLine=Actor, Banner=Banner,RollingNumbers=Actor, BPMDisplay=Actor, DeviceList=Actor, InputList=Actor, BitmapText=BitmapText, Actor=Actor, ActorFrame=ActorFrame, ActorFrameTexture=ActorFrameTexture, Quad=Sprite, Sprite=Sprite, ActorMultiVertex=ActorMultiVertex, ActorProxy=ActorProxy, WrapperState=ActorFrame, Sound=Actor}
    local lookup, assign = methods.Lookup, methods.Assign
    local function copy_text_state(value)
        if type(value) ~= 'table' then return value end
        local result = {}
        for key, item in next, value do result[copy_text_state(key)] = copy_text_state(item) end
        return result
    end
    local current_background = methods.CurrentSongBackground
    local clone_template = methods.CloneTemplate
    local function concat_template(left, right)
        if type(right) ~= 'table' then error('actor declaration table expected') end
        local actor = clone_template(left)
        for key, value in next, right do
            if key ~= '__songlua_actor_type' and key ~= '__songlua_script_dir'
                and key ~= '__songlua_song_dir' and key ~= '__songlua_bound_methods' then
                local previous = rawget(actor, key)
                if type(previous) == 'function' and type(value) == 'function' then
                    local first, second = previous, value
                    value = function(...) first(...); return second(...) end
                end
                actor[key] = value
            end
        end
        return setmetatable(actor, getmetatable(left))
    end
    local native_methods = {}
    for _, class in pairs(classes) do
        for name, method in pairs(class) do
            if type(method)=='function' and getinfo(method, 'S').what=='C' then native_methods[name]=true end
        end
    end
    local function create(class, parent, attrs, template, inplace)
        local actor = inplace and attrs or {}
        for key, value in next, attrs do
            if not inplace and type(key)=='string' and key~='children' and key~='__songlua_bound_methods'
                and not (template and (key=='__songlua_def_owned' or key=='__songlua_parent'
                    or key=='__songlua_load_name' or key=='__songlua_native_name'
                    or key=='__songlua_init_commands_ran' or key=='__songlua_startup_commands_ran'
                    or key=='__songlua_startup_children_walked'
                    or key=='__songlua_startup_command_started')) then
                if template and (class=='BitmapText' or text_displays[class]) and (key=='__songlua_text_attributes'
                    or key=='__songlua_stroke_color' or key=='Text' or sub(key,1,16)=='__songlua_state_'
                    or sub(key,1,18)=='__songlua_capture_') then
                    value=copy_text_state(value)
                end
                actor[key]=value
            end
        end
        if inplace then
            for name in pairs(native_methods) do actor[name]=nil end
        end
        actor.LoadFromCurrentSongBackground=current_background()
        actor.__songlua_actor_type=class
        if not inplace and attrs._Dir ~= nil then actor.__songlua_script_dir = attrs._Dir end
        return setmetatable(actor, {__index=lookup, __newindex=assign, __concat=concat_template, __tostring=methods.Stringify})
    end
    -- Snapshot the complete XNode-shaped input before running any Init callback.
    -- Array entries precede next traversal; the native duplicate range counts
    -- table entries, not array slots. Ancestors are skipped, shared siblings copied.
    local function snapshot(source, active)
        local attrs, entries, nodes = {}, {}, {}
        local i = 1
        while rawget(source, i) ~= nil do
            local value = rawget(source, i)
            if type(value) == 'table' then entries[#entries+1] = {tostring(i), value} end
            i = i + 1
        end
        local prefix = #entries
        for key, value in next, source do
            if type(key) ~= 'string' and type(key) ~= 'number' then
                error('actor declaration keys must be strings or numbers')
            end
            if type(key)=='string' and key~='children' then attrs[key]=value end
            if type(value) == 'table' then
                local index = tonumber(key)
                if not (index and index >= 1 and index <= prefix and index % 1 == 0) then
                    entries[#entries+1] = {tostring(key), value}
                end
            else
                attrs[tostring(key)] = value
            end
        end
        for _, entry in ipairs(entries) do
            local key, value = entry[1], entry[2]
            if not active[value] then
                active[value] = true
                nodes[#nodes+1] = {key, snapshot(value, active)}
                active[value] = nil
            end
        end
        return {attrs, nodes, source}
    end
    local function group(actors)
        return setmetatable(actors, {__songlua_child_group=true, __index=function(t, key)
            if tonumber(key) then return rawget(t, key) end
            -- Production resolves the last child's instance member at lookup,
            -- then resolves it again at invocation and forwards every result.
            local actor = rawget(t, #t)
            if not actor then return nil end
            local member = actor[key]
            if type(member)~='function' then return member end
            return function(self, ...)
                local last=self[#self]
                return last[key](last, ...)
            end
        end})
    end
    local function raw_children(actor)
        local source=rawget(actor,'children')
        local array_only=type(source)~='table'
        if array_only then source=actor end
        local entries={}
        local i=1
        while rawget(source,i)~=nil do
            local child=rawget(source,i)
            if type(child)=='table' then entries[#entries+1]={i,child} end
            i=i+1
        end
        local prefix=#entries
        for key,child in next,source do
            if type(child)=='table' then
                local kind=type(key)
                local n=(kind=='number' or kind=='string') and tonumber(key)
                if not(n and n>=1 and n<=prefix and n%1==0) then entries[#entries+1]={key,child} end
            end
        end
        local selected={}
        for _,entry in ipairs(entries) do
            local key,child=entry[1],entry[2]
            if not array_only or ((type(key)=='number' or type(key)=='string') and match(tostring(key),'^%d+$')) then
                if rawget(child,'Condition')~=false then selected[#selected+1]=child end
            end
        end
        return selected
    end
    local function offline_children(actor)
        local source=rawget(actor,'children')
        local array_only=type(source)~='table'
        if array_only then source=actor end
        local entries={}
        local i=1
        while rawget(source,i)~=nil do
            local child=rawget(source,i)
            if type(child)=='table' then entries[#entries+1]={i,child} end
            i=i+1
        end
        local prefix=#entries
        for key,child in next,source do
            if type(child)=='table' then
                local kind=type(key)
                local n=(kind=='number' or kind=='string') and tonumber(key)
                if not(n and n>=1 and n<=prefix and n%1==0) then entries[#entries+1]={key,child} end
            end
        end
        local selected={}
        for _,entry in ipairs(entries) do
            local key=entry[1]
            if not array_only or ((type(key)=='number' or type(key)=='string') and match(tostring(key),'^%d+$')) then
                selected[#selected+1]=entry
            end
        end
        return source,selected
    end
    local excluded = {
        __songlua_def_owned=true, __songlua_parent=true, __songlua_load_name=true,
        __songlua_native_name=true, __songlua_init_commands_ran=true,
        __songlua_startup_commands_ran=true, __songlua_startup_command_started=true,
        __songlua_bound_methods=true,
    }
    local mutable = {
        Frames=true, __songlua_visible=true, __songlua_wrappers=true,
        __songlua_hibernate_seconds=true, __songlua_theme_hibernating=true,
        __songlua_update_rate=true, __songlua_current_x=true, __songlua_current_y=true,
        __songlua_current_z=true, __songlua_diffuse=true, __songlua_text_attributes=true,
        __songlua_stroke_color=true, __songlua_aux=true, __songlua_stream_width=true,
        __songlua_sprite_animation_length_seconds=true, __songlua_sprite_effect_mode=true,
        Text=true, Texture=true, File=true, StreamWidth=true,
    }
    local function mutable_key(key)
        if mutable[key] then return true end
        for _, prefix in ipairs{'__songlua_state_', '__songlua_capture_', '__songlua_graph_display_',
            '__songlua_scroller_', '__songlua_aft_', '__songlua_command_queue', '__songlua_recurring_update_'} do
            if sub(key,1,#prefix)==prefix then return true end
        end
        return false
    end
    local function offline_copy(source)
        local actor={}
        for key,value in next,source do
            if not excluded[key] then
                if type(key)=='string' and mutable_key(key) then value=copy_text_state(value) end
                actor[key]=value
            end
        end
        return actor
    end
    return create, function(source) return snapshot(source, {}) end, group,
        function(callback)
            local info = getinfo(callback, 'S')
            return info and sub(info.source, 1, 7) == '@theme:'
        end, globals, tonumber, function(t)
            local entries={}
            for key,value in next,t do entries[#entries+1]={key,value} end
            return entries
        end, setmetatable({}, {__mode='k'}), setmetatable, raw_children, offline_children, offline_copy, function(actor)
            local bound=rawget(actor,'__songlua_bound_methods')
            if bound then for name in pairs(bound) do actor[name]=nil end end
        end, function()
            local info=getinfo(3,'S')
            return info and info.source
        end, copy_text_state
end
