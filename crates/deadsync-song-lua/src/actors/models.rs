//! Model resources and the existing preparation-time material clocks.
use super::*;
use std::path::PathBuf;

pub(super) fn initialize(
    call: &mut HostCall<'_>,
    actor: TableId,
    template: bool,
) -> Result<(), Error> {
    let methods = native_children::helper_function(call, b"model_methods")?;
    let methods = table(call, methods)?;
    let bound = get(call, actor, b"__songlua_bound_methods")?;
    let bound = table(call, bound)?;
    for &name in sprites::SPRITE_METHODS
        .iter()
        .chain(["GetChild", "GetChildren", "GetNumChildren"].iter())
    {
        let method = get(call, methods, name.as_bytes())?;
        let captures = [
            call.captures()[0],
            call.captures()[1],
            Value::Table(actor),
            method,
        ];
        let function = call.new_host_function_with_data(invoke_bound, &captures, name)?;
        sprites::set(call, bound, name, function)?;
    }
    if !template {
        let guard = call.store();
        let state = &mut guard.actors.get_mut(&actor).unwrap().state.render;
        state.depth_test = true;
        state.depth_write = true;
        state.texture_wrapping = true;
        state.cull_mode = deadlib_render_core::CullMode::Back;
    }
    Ok(())
}

pub(super) fn paths(
    call: &mut HostCall<'_>,
    actor: TableId,
) -> Result<Option<[PathBuf; 3]>, Error> {
    let mut paths = [None, None, None];
    for (index, key) in [b"Meshes".as_slice(), b"Materials", b"Bones"]
        .into_iter()
        .enumerate()
    {
        let Some(raw) = assets::field(call, actor, key)?.filter(|path| !path.trim().is_empty())
        else {
            continue;
        };
        let path = assets::resolve(call, actor, &raw)?.ok_or_else(|| {
            call.error(format!("actor asset '{raw}' could not be resolved").as_bytes())
        })?;
        paths[index] = Some(path);
    }
    let Some(fallback) = paths.iter().flatten().next().cloned() else {
        return Ok(None);
    };
    Ok(Some(
        paths.map(|path| path.unwrap_or_else(|| fallback.clone())),
    ))
}

pub(super) fn tap_model(
    call: &mut HostCall<'_>,
    actor: TableId,
) -> Result<Option<crate::lua_util::SongLuaNoteskinTapActorModel>, Error> {
    let Some(skin) = assets::field(call, actor, b"__songlua_noteskin_name")?
        .filter(|skin| !skin.trim().is_empty())
    else {
        return Ok(None);
    };
    let Some(element) = assets::field(call, actor, b"__songlua_noteskin_element")?
        .filter(|element| element.eq_ignore_ascii_case("Tap Note"))
    else {
        return Ok(None);
    };
    let Some(model_paths) = paths(call, actor)? else {
        return Ok(None);
    };
    Ok(Some(crate::lua_util::SongLuaNoteskinTapActorModel {
        skin,
        element,
        model_paths,
    }))
}

pub(super) fn advance_materials(store: &mut Store, actor: TableId, delta: f32) {
    if let Some(actor) = store.actors.get_mut(&actor) {
        for clock in actor.model_materials.iter_mut().flatten() {
            clock.update(delta);
        }
    }
}

impl Actors {
    pub(crate) fn model_paths(
        &self,
        vm: &mut Vm,
        actor: Value,
    ) -> Result<Option<[PathBuf; 3]>, Error> {
        vm.store().model_paths_output = None;
        let function = vm.rooted(self.read_model_paths)?;
        let result = vm.call(function, &[actor]).map(|_| ());
        let paths = vm.store().model_paths_output.take();
        result?;
        Ok(paths)
    }

    pub(crate) fn install_model_materials<V>(
        &self,
        vm: &mut Vm,
        actor: Value,
        layers: &[crate::SongLuaOverlayModelLayer<V>],
    ) -> Result<(), String> {
        let Value::Table(actor) = actor else {
            return Err("model material owner must be an actor table".into());
        };
        let store = vm.store();
        let actor = store
            .actors
            .get_mut(&actor)
            .ok_or("model material owner has been released or belongs to another runtime")?;
        let state = actor.state.render.sprite_state_index;
        actor.model_materials = layers
            .iter()
            .map(|layer| crate::model_texture::Clock::with_initial_state(layer, state))
            .collect();
        Ok(())
    }

    pub(crate) fn sample_model_materials(&self, vm: &mut Vm, second: f32) {
        for actor in vm.store().actors.values_mut() {
            for clock in actor.model_materials.iter_mut().flatten() {
                clock.sample(second);
            }
        }
    }

    pub(crate) fn take_model_material_samples(
        &self,
        vm: &mut Vm,
        actor: Value,
    ) -> Result<Vec<Arc<[crate::SongLuaTextureSample]>>, String> {
        let Value::Table(actor) = actor else {
            return Err("model material owner must be an actor table".into());
        };
        let store = vm.store();
        let actor = store
            .actors
            .get_mut(&actor)
            .ok_or("model material owner has been released or belongs to another runtime")?;
        Ok(std::mem::take(&mut actor.model_materials)
            .into_iter()
            .map(|clock| clock.map_or_else(|| Arc::from([]), crate::model_texture::Clock::finish))
            .collect())
    }
}
