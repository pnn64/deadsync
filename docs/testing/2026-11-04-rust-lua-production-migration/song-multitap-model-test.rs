#[test]
fn update_stage_installs_model_clocks_from_compiled_initial_state() {
    use deadsync_song_lua::{SongLuaOverlayModelLayer,SongLuaOverlayModelDraw,SongLuaModelTextureFrame,SongLuaModelTextureUv};
    let mut pair=Pair::new(&factory(true,1,false), |_|{});
    let index=pair.rows.actors.iter().position(|r|r.actor.name.as_deref()==Some("MultitapArrowP1_1")).unwrap();
    let uv=SongLuaModelTextureUv{scale:[1.;2],offset:[0.;2],shift:[0.;2]};
    let mut layer=SongLuaOverlayModelLayer::new(Arc::from("material fixture"),Arc::<[()]>::from([]),[2.;2],[1.;2],[0.;2],[0.;2],[1.,0.5],None,
        SongLuaOverlayModelDraw{pos:[0.;3],rot:[0.;3],zoom:[1.;3],tint:[1.;4],vert_align:0.,blend_add:false,visible:true});
    layer.texture_frames=Arc::from([
        SongLuaModelTextureFrame{texture_key:None,delay:0.02,diffuse:uv,glow:uv},
        SongLuaModelTextureFrame{texture_key:None,delay:0.03,diffuse:SongLuaModelTextureUv{offset:[0.25,0.5],..uv},glow:uv},
    ]);
    let kind=SongLuaOverlayKind::Model{layers:Arc::from([layer])};
    pair.native_rows[index].actor.kind=kind.clone();
    pair.rows.actors[index].actor.kind=kind;
    // Preparation can rewind a startup texture state independently of the
    // actor's current exported getter. Native installation uses compiled state.
    pair.native_rows[index].actor.initial_state.sprite_state_index=Some(1);
    pair.rows.actors[index].actor.initial_state.sprite_state_index=Some(1);
    pair.native.globals().set("model_probe",pair.native_rows[index].table.clone()).unwrap();
    pair.stage();
    let a=test_support::take_legacy_model_material_samples(&pair.native,"model_probe").unwrap();
    let actor=pair.rust.song_overlay_actor_value(&pair.rows.actors[index]).unwrap();
    let b=pair.rust.take_actor_model_material_samples(actor).unwrap();
    assert_eq!(a.len(),b.len());
    assert!(!a[0].is_empty());
    assert_eq!(a[0][0].diffuse_state,1);
    for (a,b) in a.iter().zip(&b) {
        assert_eq!(a.len(),b.len());
        for (a,b) in a.iter().zip(b.iter()) {
            assert_eq!(a.second,b.second);
            assert_eq!(a.diffuse_state,b.diffuse_state);
            assert_eq!(a.additive_state,b.additive_state);
            assert_eq!(a.diffuse,b.diffuse);
            assert_eq!(a.glow,b.glow);
            assert_eq!(a.additive,b.additive);
        }
    }
    pair.finish();
}
