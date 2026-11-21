#![cfg(feature = "test-support")]

use deadlib_lua::{Value, compile_named};
use deadlib_present::actors::TextAttribute;
use deadsync_song_lua::{self as song, ActorScene, SongLuaLiveTarget as Target, SongRuntime};
use std::{path::Path, sync::Arc};

fn execute(runtime: &mut SongRuntime, source: &str) -> Vec<Value> {
    runtime
        .vm_mut()
        .execute(&compile_named(source.as_bytes(), b"@/fixture/wrapper-removal.lua").unwrap())
        .unwrap()
        .to_vec()
}

fn actor_scene(source: &str) -> (SongRuntime, ActorScene) {
    let mut context = song::SongLuaCompileContext::new(".", "wrapper removal");
    context.live = true;
    let mut runtime = SongRuntime::new(&context).unwrap();
    let definition = execute(&mut runtime, source)[0];
    let scene = runtime.instantiate_actor(definition).unwrap();
    let root = runtime.actor_scene_root(scene).unwrap();
    runtime.vm_mut().set_global(b"root", root).unwrap();
    (runtime, scene)
}

fn advance(runtime: &mut SongRuntime, scene: ActorScene, seconds: f32, delta: f64) {
    runtime.set_values(seconds * 2., seconds).unwrap();
    assert!(runtime.advance_actor_scene(scene, delta).unwrap());
    assert!(runtime.take_actor_diagnostics().is_empty());
}

#[test]
fn removal_validates_native_indices_and_returns_the_owner() {
    // Actor.cpp get_state_index uses IArg (luaL_checkint), then one-based bounds.
    // Lua 5.1 accepts numeric strings and truncates finite fractional indices.
    let (mut runtime, scene) = actor_scene("return Def.ActorFrame{Def.Quad{Name='Child'}}");
    execute(
        &mut runtime,
        r#"
        first=root:AddWrapperState():x(10)
        middle=root:AddWrapperState():x(20)
        last=root:AddWrapperState():x(30)
        child=root:GetChild('Child')
        middle.saved='raw field survives deletion'
        local middle_x=middle.x
        for _,index in ipairs{0,-1,4,false,{},'not a number'} do
            assert(not pcall(root.RemoveWrapperState,root,index))
            assert(root:GetNumWrapperStates()==3)
            assert(root:GetWrapperState(1)==first and root:GetWrapperState(2)==middle
                and root:GetWrapperState(3)==last)
        end
        assert(not pcall(root.RemoveWrapperState,root))
        assert(root:GetNumWrapperStates()==3)
        assert(root:RemoveWrapperState('2')==root)
        assert(root:GetNumWrapperStates()==2)
        assert(root:GetWrapperState(1)==first and root:GetWrapperState(2)==last)
        assert(middle.saved=='raw field survives deletion')
        assert(not pcall(middle.GetX,middle))
        assert(not pcall(middle_x,middle,99))
        assert(root:RemoveWrapperState(1.9)==root)
        assert(root:GetNumWrapperStates()==1 and root:GetWrapperState(1)==last)
        assert(root:RemoveWrapperState('1.9')==root)
        assert(root:GetNumWrapperStates()==0)
        assert(root:GetChild('Child')==child and root:GetNumChildren()==1)
        assert(not pcall(root.RemoveWrapperState,root,1))
        collectgarbage('collect')
        assert(middle.saved=='raw field survives deletion')
    "#,
    );
    assert!(runtime.take_actor_diagnostics().is_empty());
    assert!(runtime.release_actor_scene(scene).unwrap());
}

#[test]
fn removal_retires_nested_wrapper_queues_updates_and_subscriptions() {
    // Actor::~Actor stops queues, unsubscribes, and deletes its nested wrappers.
    let (mut runtime, scene) = actor_scene("return Def.ActorFrame{}");
    execute(
        &mut runtime,
        r#"
        updates=0; pings=0; queued=0; survivor_updates=0
        removed=root:AddWrapperState()
        nested=removed:AddWrapperState()
        survivor=root:AddWrapperState()
        for _,wrapper in ipairs{removed,nested} do
            wrapper.raw_value='retained table'
            wrapper:SetUpdateFunction(function() updates=updates+1 end)
            wrapper.PingMessageCommand=function() pings=pings+1 end
            wrapper.QueuedCommand=function() queued=queued+1 end
            wrapper:sleep(.25):queuecommand('Queued')
        end
        survivor:SetUpdateFunction(function() survivor_updates=survivor_updates+1 end)
    "#,
    );
    advance(&mut runtime, scene, 0.125, 0.125);
    execute(
        &mut runtime,
        r#"
        MESSAGEMAN:Broadcast('Ping')
        assert(updates==2 and pings==2 and queued==0 and survivor_updates==1)
        assert(root:RemoveWrapperState(1)==root)
        assert(root:GetNumWrapperStates()==1 and root:GetWrapperState(1)==survivor)
        assert(not pcall(removed.GetX,removed))
        assert(not pcall(nested.GetX,nested))
        assert(removed.raw_value=='retained table' and nested.raw_value=='retained table')
        collectgarbage('collect')
    "#,
    );
    advance(&mut runtime, scene, 0.625, 0.5);
    execute(
        &mut runtime,
        r#"
        MESSAGEMAN:Broadcast('Ping')
        assert(updates==2 and pings==2 and queued==0)
        assert(survivor_updates==2)
    "#,
    );
    let snapshot = runtime.actor_scene_snapshot(scene).unwrap();
    assert_eq!(
        snapshot.len(),
        2,
        "only the owner and surviving wrapper remain owned"
    );
    assert_eq!(snapshot[0].wrappers.len(), 1);
    assert!(runtime.release_actor_scene(scene).unwrap());
}

#[test]
fn broadcast_skips_a_wrapper_removed_by_an_earlier_subscriber() {
    let (mut runtime, scene) = actor_scene("return Def.ActorFrame{}");
    execute(
        &mut runtime,
        r#"
        delivered=''
        removed=root:AddWrapperState()
        survivor=root:AddWrapperState()
        root.RemoveMessageCommand=function(self)
            delivered=delivered..'owner;'
            self:RemoveWrapperState(1)
        end
        removed.RemoveMessageCommand=function()
            error('deleted wrapper subscriber was still dispatched')
        end
        survivor.RemoveMessageCommand=function()
            delivered=delivered..'survivor;'
        end
    "#,
    );
    // Native subscribers are ordered by pointer. Fix that independent input so
    // removal always precedes the deleted wrapper's pending delivery here.
    assert!(runtime.set_actor_message_ranks(scene, &[1, 2, 3]));
    execute(
        &mut runtime,
        r#"
        MESSAGEMAN:Broadcast('Remove')
        assert(delivered=='owner;survivor;')
        assert(root:GetNumWrapperStates()==1 and root:GetWrapperState(1)==survivor)
        assert(not pcall(removed.GetX,removed))
    "#,
    );
    assert!(runtime.take_actor_diagnostics().is_empty());
    assert!(runtime.release_actor_scene(scene).unwrap());
}

#[test]
fn queued_owner_update_continues_after_removing_both_wrappers() {
    // Pernicious Deed removes 2 then 1 inside UpdateCommand and reschedules at
    // its end. An unsupported removal used to suppress its later Hurt action.
    let (mut runtime, scene) = actor_scene(
        r#"
        updates=0
        return Def.ActorFrame{
            OnCommand=function(self)
                self:AddWrapperState()
                self:AddWrapperState()
                self:queuecommand('Update')
            end,
            UpdateCommand=function(self)
                updates=updates+1
                if updates==1 then
                    assert(self:RemoveWrapperState(2):RemoveWrapperState(1)==self)
                    self:sleep(.1):queuecommand('Update')
                else
                    self:GetChild('Target'):vibrate()
                end
            end,
            Def.Quad{Name='Target'},
        }
    "#,
    );
    assert!(runtime.start_actor_scene(scene).unwrap());
    advance(&mut runtime, scene, 0.05, 0.05);
    execute(
        &mut runtime,
        "assert(updates==1 and root:GetNumWrapperStates()==0)",
    );
    advance(&mut runtime, scene, 0.15, 0.1);
    execute(
        &mut runtime,
        "assert(updates==2 and root:GetNumWrapperStates()==0)",
    );
    let states = runtime.actor_scene_snapshot(scene).unwrap();
    let target = states.iter().find(|actor| actor.name == b"Target").unwrap();
    assert!(
        target.render.vibrate,
        "the later authored callback reached its actor"
    );
    assert!(runtime.release_actor_scene(scene).unwrap());
}

type Kind = song::SongLuaOverlayKind<(), (), TextAttribute>;
type Actor = song::SongLuaOverlayActor<Kind>;
type Visuals = deadsync_gameplay::SongLuaRuntimeVisuals<
    Actor,
    song::SongLuaCapturedActor,
    song::gameplay::SongLuaRuntimeOverlayStateDelta,
>;

fn slots(_: &Path, _: &Path, _: &Path) -> Result<Arc<[()]>, String> {
    Ok(Arc::from([]))
}

fn layer(_: &()) -> Option<song::SongLuaOverlayModelLayer<()>> {
    None
}

fn visuals(output: song::CompiledSongLua<Actor>) -> Visuals {
    Visuals {
        overlays: output.overlays,
        screen_overlay_index: output.screen_overlay_index,
        overlay_eases: Vec::new(),
        overlay_ease_ranges: Vec::new(),
        overlay_events: Vec::new(),
        background_visual_layers: Vec::new(),
        foreground_visual_layers: Vec::new(),
        player_actors: output.player_actors,
        player_events: Default::default(),
        player_judgment_events: Default::default(),
        player_combo_events: Default::default(),
        song_foreground: output.song_foreground,
        song_foreground_events: Vec::new(),
        hidden_players: output.hidden_players,
        screen_layers: output.screen_layers,
        screen_layer_events: Default::default(),
        note_hides: Default::default(),
        column_offsets: Default::default(),
        column_splines: Default::default(),
        screen_width: output.screen_width,
        screen_height: output.screen_height,
    }
}

fn update(scene: &mut song::SongLiveScene, seconds: f64, delta: f64) {
    let output = scene
        .update(song::SongLuaLiveFrame {
            music_seconds: seconds,
            screen_seconds: seconds,
            beat: seconds * 2.,
            delta_seconds: delta,
            ..Default::default()
        })
        .unwrap();
    assert!(output.errors.is_empty(), "{:?}", output.errors);
}

#[test]
fn middle_removal_preserves_visual_slots_and_refreshes_surviving_wrapper_paths() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("main.lua");
    std::fs::write(
        &path,
        r#"
        return Def.ActorFrame{Name='Container',
            OnCommand=function(self) self:sleep(1000) end,
            Def.Quad{Name='Owner', InitCommand=function(self) OWNER=self end,
                OnCommand=function(self)
                    ONE=self:AddWrapperState():x(10)
                    TWO=self:AddWrapperState():y(20)
                    THREE=self:AddWrapperState():z(30)
                end},
            Def.Quad{Name='Sibling'},
        }
    "#,
    )
    .unwrap();
    let mut context = song::SongLuaCompileContext::new(directory.path(), "wrapper removal visuals");
    context.live = true;
    context.defer_initial_live_update = true;
    context.music_length_seconds = 10.;
    context.players[0].step_statistics_enabled = false;
    context.players[1].enabled = false;
    let mut output = song::compile_song_lua_layers(
        &[&path],
        0,
        &context,
        Default::default(),
        slots,
        layer,
        |_, _| None,
    )
    .unwrap()
    .remove(0);
    let mut scene = output.live_scene.take().unwrap();
    assert!(scene.take_startup_output().errors.is_empty());
    scene
        .bind_targets(
            output
                .live_actor_indices()
                .iter()
                .copied()
                .enumerate()
                .map(|(local, global)| (global, Target::Root(local)))
                .collect(),
        )
        .unwrap();
    let mut visual = visuals(output);
    let owner = visual
        .overlays
        .iter()
        .position(|actor| actor.name.as_deref() == Some("Owner"))
        .unwrap();
    let sibling = visual
        .overlays
        .iter()
        .position(|actor| actor.name.as_deref() == Some("Sibling"))
        .unwrap();
    let container = visual
        .overlays
        .iter()
        .position(|actor| actor.name.as_deref() == Some("Container"))
        .unwrap();
    let original_parent = Some(container);
    update(&mut scene, 0.1, 0.1);
    scene.write_visuals(&mut visual).unwrap();
    let slots_before = visual.overlays.len();
    let bindings_before = scene.visual_draw_bindings().len();
    let epoch_before = scene.visual_binding_epoch();
    let first = visual.overlays[owner].parent_index.unwrap();
    let middle = visual.overlays[first].parent_index.unwrap();
    let last = visual.overlays[middle].parent_index.unwrap();
    assert_eq!(visual.overlays[last].parent_index, original_parent);
    let owner_path = scene
        .visual_bindings()
        .unwrap()
        .into_iter()
        .find_map(|(path, target)| {
            matches!(target, Target::Root(index) if index == owner).then_some(path)
        })
        .unwrap();
    {
        let mut runtime = scene.runtime_mut();
        execute(
            &mut runtime,
            r#"
            assert(OWNER:GetWrapperState(1)==ONE and OWNER:GetWrapperState(2)==TWO
                and OWNER:GetWrapperState(3)==THREE)
            assert(OWNER:RemoveWrapperState(2)==OWNER)
            assert(OWNER:GetWrapperState(1)==ONE and OWNER:GetWrapperState(2)==THREE)
            assert(not pcall(TWO.GetX,TWO))
            collectgarbage('collect')
        "#,
        );
    }
    update(&mut scene, 0.2, 0.1);
    scene.write_visuals(&mut visual).unwrap();
    assert_eq!(
        visual.overlays.len(),
        slots_before,
        "removal cannot shift prepared visual slots"
    );
    assert_eq!(
        scene.visual_draw_bindings().len(),
        bindings_before,
        "retired slots retain their independent global/local identities"
    );
    assert_ne!(
        scene.visual_binding_epoch(),
        epoch_before,
        "a topology change must be observable without a binding-count change"
    );
    assert_eq!(visual.overlays[owner].name.as_deref(), Some("Owner"));
    assert_eq!(visual.overlays[sibling].name.as_deref(), Some("Sibling"));
    assert_eq!(visual.overlays[owner].parent_index, Some(first));
    assert_eq!(visual.overlays[first].parent_index, Some(last));
    assert_eq!(visual.overlays[last].parent_index, original_parent);
    assert_eq!(visual.overlays[middle].parent_index, None);
    assert_eq!(visual.overlays[first].initial_state.x, 10.);
    assert_eq!(visual.overlays[last].initial_state.z, 30.);
    let paths = scene.visual_bindings().unwrap();
    for (ordinal, slot) in [(1, first), (2, last)] {
        assert!(paths.iter().any(|(path, target)| path
            == &format!("{owner_path}/WrapperState{ordinal}")
            && matches!(target, Target::Root(index) if *index == slot)));
    }
    assert!(
        !paths
            .iter()
            .any(|(_, target)| matches!(target, Target::Root(index) if *index == middle))
    );
    assert!(
        !paths
            .iter()
            .any(|(path, _)| path == &format!("{owner_path}/WrapperState3"))
    );
    let epoch_after = scene.visual_binding_epoch();
    scene.write_visuals(&mut visual).unwrap();
    assert_eq!(
        scene.visual_binding_epoch(),
        epoch_after,
        "publication is idempotent"
    );
    assert!(scene.end().unwrap().errors.is_empty());
}
