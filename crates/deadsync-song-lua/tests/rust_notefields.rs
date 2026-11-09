use deadlib_lua::{Value, compile_named};
use deadsync_song_lua::{
    SongLuaCompileContext,
    rust_engine::{ActorScene, RustSongRuntime, test_support},
};
use std::path::Path;

struct Pair {
    rust: RustSongRuntime,
    scene: ActorScene,
}
impl Pair {
    fn new(style: &str, live: bool) -> Self {
        let mut context = SongLuaCompileContext::new(Path::new("."), "NoteField migration");
        context.live = live;
        context.style_name = style.into();

        let mut rust = RustSongRuntime::new(&context).unwrap();
        let scene = test_support::note_field(&mut rust, 1, style).unwrap();
        let actor = rust.actor_scene_root(scene).unwrap();
        rust.vm_mut().set_global(b"nf", actor).unwrap();
        let mut p = Self { rust, scene };
        p.script(
            r#"
            observations={};function log(...)
                local t={};for i=1,select('#',...) do local v=select(i,...)
                    if type(v)=='number' then
                        t[i]=v~=v and 'nan' or string.format('%.9g',v)
                    else t[i]=tostring(v) end
                end;observations[#observations+1]=table.concat(t,',')
            end
            function vector(v)log(v[1],v[2],v[3])end
            columns=nf:GetColumnActors();c=columns[1];h=c:GetPosHandler();s=h:GetSpline()
        "#,
        );
        p
    }
    fn script(&mut self, source: &str) {
        self.rust
            .vm_mut()
            .execute(&compile_named(source.as_bytes(), b"@/fixture/notefield.lua").unwrap())
            .unwrap_or_else(|e| panic!("Rust: {source}\n{e:?}"));
        let Value::String(actual) = self
            .rust
            .vm_mut()
            .execute(
                &compile_named(b"return table.concat(observations,'|')", b"=observations").unwrap(),
            )
            .unwrap()[0]
        else {
            panic!()
        };
        reference_test::assert_debug(
            "song/tests/rust_notefields.rs::observations",
            &self.rust.vm_mut().string_bytes(actual).unwrap(),
        );
    }
}

#[test]
fn construction_eagerly_owns_style_columns_board_and_spline_handlers() {
    for style in ["single", "double", "solo", "pump-single", "pump-double"] {
        for live in [false, true] {
            let mut p = Pair::new(style, live);
            p.script(r#"
                assert(nf.__songlua_note_columns==columns and nf:get_column_actors()==columns)
                assert(nf:GetNumChildren()==1 and nf[1]==nf:GetChild('Board'))
                assert(nf[1]:GetParent()==nf and nf[1]:GetName()=='Board')
                assert(nf:GetChild('missing')==nil and nf:GetParent()==nil)
                assert(nf.__songlua_player_index==1 and nf.__songlua_player_child_name=='NoteField')
                assert(not nf.__songlua_beat_bars and next(nf.__songlua_beat_bars_alpha)==nil)
                log(#columns,nf:GetX(),nf:GetY(),nf:GetZ(),nf[1]:GetWidth(),nf[1]:GetHeight())
                log(nf[1]:GetTexture():GetPath(),nf[1]:GetTexture():GetTextureWidth())
                for i,column in ipairs(columns) do
                    assert(column:GetParent()==nf and column.__songlua_player_index==1)
                    assert(column.__songlua_column_index==i-1 and column.SetUpdateFunction==nil)
                    assert(column.GetPosHandler==column.get_pos_handler)
                    assert(column.GetRotHandler==column.get_rot_handler)
                    assert(column.GetZoomHandler==column.get_zoom_handler)
                    assert(column:GetPosHandler()~=column:GetRotHandler())
                    assert(column:GetRotHandler()~=column:GetZoomHandler())
                    log(column:GetX(),column:GetY(),column.__songlua_state_x,column:GetName())
                end
                assert(h.GetSpline==h.get_spline and h:GetSpline()==s)
                assert(h.__songlua_spline==s and h.__songlua_spline_mode=='NoteColumnSplineMode_Disabled')
                assert(h.__songlua_subtract_song_beat and h.__songlua_receptor_t==0 and h.__songlua_beats_per_t==1)
                assert(s:GetSize()==0 and not s:get_loop());vector(s:evaluate(0))
                assert(s.Evaluate==s.evaluate and s.SetSize==s.set_size and s.Solve==s.solve)
            "#);
        }
    }
}

#[test]
fn field_methods_keep_bound_receivers_alias_lookup_and_argument_rules() {
    let mut p = Pair::new("double", true);
    p.script(
        r#"
        assert(type(NoteField)=='table' and getmetatable(NoteField).__index==ActorFrame)
        for _,name in ipairs{'set_step_callback','set_set_pressed_callback','set_did_hold_note_callback',
            'step','set_pressed','did_hold_note','GetBeatBars'} do
            assert(type(NoteField[name])=='function' and not pcall(NoteField[name],nf))
        end
        assert(not pcall(nf.SetBeatBars) and not pcall(nf.SetBeatBars,{}))
        local f=nf.__songlua_bound_methods.SetBeatBars;local a=nf.__songlua_bound_methods.SetBeatBarsAlpha
        __songlua_side_effect_count=0
        assert(f()==nf and __songlua_side_effect_count==0)
        assert(f(nil)==nf and not nf.__songlua_beat_bars)
        assert(f({},0)==nf and nf.__songlua_beat_bars)
        assert(a({},.125,'0.25','0x10',false)==nf)
        for i=1,4 do log(nf.__songlua_beat_bars_alpha[i])end
        a('nan','inf',math.huge,1e100)
        for i=1,4 do log(nf.__songlua_beat_bars_alpha[i])end
        local upper=nf.GetColumnActors;local lower=nf.__songlua_bound_methods.get_column_actors
        assert(upper({},'ignored')==columns and lower()==columns)
        nf.GetColumnActors=function(self)assert(self==nf);collectgarbage('collect');return 17 end
        assert(lower({})==17);nf.GetColumnActors=false;assert(lower()==nil)
        nf.GetColumnActors=nil;assert(lower()==columns)
        nf.__songlua_player_child_name='wrong';nf.__songlua_actor_type='Actor'
        assert(upper()==nil)
        nf.__songlua_player_child_name='NOTEfield';assert(upper()==columns)
        nf.__songlua_actor_type='NoteField'
        nf.__songlua_note_columns=false;assert(not pcall(upper))
        nf.__songlua_note_columns=nil;nf.__songlua_style_name='single'
        local fresh=upper();assert(fresh~=columns and #fresh==4 and #columns==8)
        assert(columns[1]:GetPosHandler()==h and fresh[1]:GetPosHandler()~=h)
        log(__songlua_side_effect_count)
    "#,
    );
}

#[test]
fn handler_setters_use_argument_two_preserve_aliases_and_original_spline() {
    let mut p = Pair::new("single", false);
    p.script(r#"
        assert(h:SetSplineMode('NoteColumnSplineMode_Position')==h)
        h.SetSplineMode('ignored');assert(h.__songlua_spline_mode=='NoteColumnSplineMode_Position')
        h.SetSplineMode(false,'valid');assert(h.__songlua_spline_mode=='valid')
        h:SetSplineMode(9);h:SetSplineMode(string.char(255));assert(h.__songlua_spline_mode=='valid')
        for _,v in ipairs{false,0,'',{},true} do
            assert(h:SetSubtractSongBeat(v)==h);log(h.__songlua_subtract_song_beat)
        end
        h.SetSubtractSongBeat(true);assert(not h.__songlua_subtract_song_beat)
        for _,v in ipairs{.125,'.375','0x10',false,{},math.huge,1e100,'nan','inf','-inf'} do
            h:SetReceptorT(v);h:SetBeatsPerT(v);log(h.__songlua_receptor_t,h.__songlua_beats_per_t)
        end
        local saved=h.GetSpline;h.__songlua_spline={};assert(saved()==s)
        assert(h.set_receptor_t==h.SetReceptorT and h.set_beats_per_t==h.SetBeatsPerT)
        assert(h.set_spline_mode==h.SetSplineMode and h.set_subtract_song_beat==h.SetSubtractSongBeat)
    "#);
}

#[test]
fn cubic_solving_looping_resize_and_float_evaluation_match_existing_geometry() {
    for live in [false, true] {
        let mut p = Pair::new("single", live);
        p.script(r#"
            for _,loop in ipairs{false,true} do
                assert(s:SetLoop(loop)==s and s:get_loop()==loop)
                for _,count in ipairs{0,1,2,3,5,8} do
                    s:SetSize(count)
                    for i=1,count do s:SetPoint(i,{i*7-9,1/(i+1),i%3-1})end
                    assert(s:Solve()==s and s:SetPolygonal(true)==s)
                    for _,t in ipairs{-4,-1.5,-.5,0,.125,.5,.999,1,2.5,7.5,10,1000} do vector(s:evaluate(t))end
                    for i=1,count do for axis=1,3 do vector(s.__songlua_spline_coefficients[i][axis])end end
                end
            end
            s:SetLoop(false):SetSize(4):SetPoint(1,{1,2,3}):SetPoint(2,{4,5,6}):Solve()
            local points=s.__songlua_spline_points;local coefficients=s.__songlua_spline_coefficients
            s:SetSize(2);assert(points[3]==nil and coefficients[3]==nil)
            s:SetSize(4);assert(points[3]==nil and coefficients[3]==nil)
            vector(s:evaluate(1.5));vector(s:evaluate(3))
            local original={8,'0x10'};s:SetPoint(1,original);original[1]=100
            vector(s.__songlua_spline_points[1]);assert(s.__songlua_spline_points[1]~=original)
            s:SetSize('2.9');assert(s:GetSize()==2)
            local before=s.__songlua_spline_points[1];s:SetPoint(1.9,{7});assert(s.__songlua_spline_points[1]~=before)
            vector(s:evaluate('0.5'));vector(s.evaluate({},'0x1'))
        "#);
    }
}

#[test]
fn invalid_geometry_rejects_before_replacing_existing_values() {
    let mut p = Pair::new("single", true);
    p.script(r#"
        s:SetSize(2):SetPoint(1,{3,4,5}):Solve()
        local points=s.__songlua_spline_points;local original=points[1]
        for _,v in ipairs{false,{},'invalid',math.huge,-math.huge,0/0,65537,-1} do
            assert(not pcall(s.SetSize,s,v));assert(s:GetSize()==2)
        end
        assert(s:SetSize(-.5)==s and s:GetSize()==0);s:SetSize(2):SetPoint(1,{3,4,5});original=points[1]
        for _,index in ipairs{0,3,-1,math.huge,0/0,'bad',false} do assert(not pcall(s.SetPoint,s,index,{1}))end
        for _,value in ipairs{false,'bad',{1,2,3,'invalid'},{false},{1,2,3,false}} do
            assert(not pcall(s.SetPoint,s,1,value));assert(points[1]==original)
        end
        for _,value in ipairs{false,{},'invalid'} do assert(not pcall(s.evaluate,s,value))end
        s:SetPoint(1,{});vector(points[1])
        points[1]=false;assert(not pcall(s.Solve) and not pcall(s.evaluate,s,0))
        points[1]={1,'bad',3};assert(not pcall(s.Solve))
        points[1]={1,2,3};s:Solve();vector(s:evaluate(.5))
        s.__songlua_spline_size='bad';assert(not pcall(s.GetSize));s.__songlua_spline_size=2
    "#);
}

#[test]
fn ordinary_field_access_reenters_and_keeps_selected_geometry_rooted() {
    let mut p = Pair::new("single", true);
    p.script(r#"
        s:SetSize(2):SetPoint(1,{1,2,3}):SetPoint(2,{4,5,6}):Solve()
        local original=s.__songlua_spline_points;local coefficients=s.__songlua_spline_coefficients
        s.__songlua_spline_points=nil;s.__songlua_spline_coefficients=nil
        setmetatable(s, {__index=function(t,k)
            collectgarbage('collect')
            if k=='__songlua_spline_points' then log('points');return original end
            if k=='__songlua_spline_coefficients' then log('coefficients');return coefficients end
        end,__newindex=function(t,k,v)
            collectgarbage('collect');log('set',k);rawset(t,k,v)
        end})
        s:SetPoint(1,{7,8,9});s:Solve();vector(s:evaluate(.5))
        s:SetSize(1);assert(original[2]==nil)
        setmetatable(s,nil);s.__songlua_spline_points=original
        h.__songlua_receptor_t=nil
        setmetatable(h,{__newindex=function(t,k,v)
            assert(k=='__songlua_receptor_t');collectgarbage('collect');t:SetBeatsPerT(.125);rawset(t,k,v)
        end})
        h:SetReceptorT(.375);assert(h.__songlua_receptor_t==.375 and h.__songlua_beats_per_t==.125)
        setmetatable(h,nil)
    "#);
}

#[test]
fn retained_handler_queries_release_field_and_keep_only_their_selected_objects() {
    let mut p = Pair::new("single", true);
    p.script(
        r#"
        weak=setmetatable({nf,c,h,s},{__mode='v'})
        saved_handler=c.GetPosHandler;saved_spline=h.GetSpline
        nf=nil;columns=nil;c=nil;h=nil;s=nil;collectgarbage('collect')
        assert(weak[1] and weak[2] and weak[3] and weak[4])
    "#,
    );
    p.rust.release_actor_scene(p.scene).unwrap();
    let code = compile_named(
        br#"
        collectgarbage('collect');assert(weak[1]==nil and weak[2]==nil)
        assert(saved_handler()==weak[3] and saved_spline()==weak[4])
        saved_handler=nil;collectgarbage('collect');assert(weak[3]==nil and weak[4])
        saved_spline=nil;collectgarbage('collect');assert(weak[4]==nil)
    "#,
        b"=retire NoteField",
    )
    .unwrap();
    p.rust.vm_mut().execute(&code).unwrap();
}

#[test]
fn tap_callbacks_keep_public_forwarders_reentry_and_partial_failure_behavior() {
    let mut p = Pair::new("single", true);
    p.script(r#"
        assert(type(nf.set_did_tap_note_callback)=='function')
        assert(nf.__songlua_bound_methods.SetDidTapNoteCallback==nf.__songlua_bound_methods.set_did_tap_note_callback)
        assert(not pcall(NoteField.did_tap_note))
        __songlua_side_effect_count=0
        local callback=function(column,score,bright)
            log('tap',column,score,bright,__songlua_side_effect_count)
            assert(nf.__songlua_last_tap_note_column==column and nf.__songlua_last_tap_note_score==score)
            assert(nf.__songlua_last_tap_note_bright==bright)
            nf:SetDidTapNoteCallback(nil);collectgarbage('collect')
            nf:did_tap_note(9,'nested',false)
            return false,'ignored'
        end
        assert(nf:set_did_tap_note_callback(callback)==nf)
        assert(nf:did_tap_note('1.5','TapNoteScore_W1',0)==nf)
        assert(__songlua_side_effect_count==2 and nf.__songlua_last_tap_note_column==9)
        assert(nf.__songlua_did_tap_note_callback==nil)
        for _,value in ipairs{false,{},'bad',math.huge,-math.huge,0/0,'nan','inf','0x10',2.5,-2.5} do
            nf:did_tap_note(value,false,nil);log(nf.__songlua_last_tap_note_column)
        end
        nf:SetDidTapNoteCallback(function()error('tap failed')end)
        local old=__songlua_side_effect_count
        assert(not pcall(NoteField.did_tap_note,nf,3,'failed',true))
        assert(nf.__songlua_last_tap_note_score=='failed' and __songlua_side_effect_count==old)
        nf:SetDidTapNoteCallback(false);assert(nf.__songlua_did_tap_note_callback==nil)
        nf.__songlua_did_tap_note_callback={};assert(not pcall(NoteField.did_tap_note,nf,4))
        nf.__songlua_did_tap_note_callback=nil
        __songlua_side_effect_count='bad';assert(not pcall(NoteField.SetBeatBars,nf,true))
        assert(nf.__songlua_beat_bars)
        __songlua_side_effect_count='2.5';nf:SetBeatBarsAlpha(.5);assert(__songlua_side_effect_count==3)
    "#);
}

#[test]
fn columns_advance_after_frame_with_original_delta_through_rate_and_hibernation() {
    let mut p = Pair::new("single", true);
    p.script(
        r#"
        root=nf
        nf:linear(1):x(100);c:linear(1):x(100)
        nf:SetUpdateRate(2)
        nf:SetUpdateFunction(function(self,dt) log('frame',dt,self:GetX(),c:GetX()) end)
        c:addcommand('Mark',function(self)log('column',self:GetX(),nf:GetX())end)
        c:queuecommand('Mark')
    "#,
    );
    for (i, delta) in [0.25, 0.25, 0.25].into_iter().enumerate() {
        if i == 1 {
            p.script("nf:hibernate(.5)");
        }

        p.rust.advance_actor_scene(p.scene, delta).unwrap();
        p.script("log('after',nf:GetX(),c:GetX())");
    }
    p.script(
        r#"
        nf.__songlua_note_columns=nil
        local replacement=nf:GetColumnActors();assert(replacement~=columns)
        replacement[1]:linear(1):x(50)
    "#,
    );

    p.rust.advance_actor_scene(p.scene, 0.25).unwrap();
    p.script("log('replacement',nf:GetColumnActors()[1]:GetX(),c:GetX())");
}

#[test]
fn regenerated_columns_follow_current_style_and_root_authored_getters() {
    let mut p = Pair::new("double", false);
    p.script(r#"
        nf.__songlua_note_columns=nil;nf.__songlua_style_name=nil
        assert(#nf:GetColumnActors()==8)
        local old=GAMESTATE
        GAMESTATE={__songlua_current_style={{__songlua_style_key='pump-single'}}}
        nf.__songlua_note_columns=nil;local a=nf:GetColumnActors();assert(#a==5)
        GAMESTATE={GetCurrentStyle=function(self)
            collectgarbage('collect');return {GetName=function()return 'pump-double' end}
        end}
        nf.__songlua_note_columns=nil;assert(#nf:GetColumnActors()==10)
        GAMESTATE={__songlua_current_style={false},GetCurrentStyle=function()error('must not run')end}
        nf.__songlua_note_columns=nil;assert(#nf:GetColumnActors()==4)
        GAMESTATE=old
        nf.__songlua_note_columns=nil;nf.__songlua_style_name=false;assert(not pcall(nf.GetColumnActors))
        nf.__songlua_style_name='single';nf.__songlua_player_index='1.9'
        local b=nf:GetColumnActors();assert(b[1].__songlua_player_index==1)
    "#);
}

#[test]
fn retired_field_query_cannot_construct_actors_in_a_released_scene() {
    let mut p = Pair::new("single", true);
    p.script("saved_columns=nf.GetColumnActors;nf.__songlua_note_columns=nil");
    p.rust.release_actor_scene(p.scene).unwrap();
    let code = compile_named(
        b"assert(not pcall(saved_columns));collectgarbage('collect')",
        b"=retired field query",
    )
    .unwrap();
    p.rust.vm_mut().execute(&code).unwrap();
}

#[allow(dead_code)]
#[path = "../../deadlib-lua/src/reference_test.rs"]
mod reference_test;
