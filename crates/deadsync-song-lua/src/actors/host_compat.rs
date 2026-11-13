//! Existing host services backed by the actor registry and owned live state.
use super::*;

impl Actors {
    pub(crate) fn install_host_compat(&self, vm: &mut Vm) -> Result<(), Error> {
        install(
            vm,
            &self.store,
            &[Value::Table(self.registry), Value::Table(self.helpers)],
        )
    }
}

pub(super) fn install(vm: &mut Vm, store: &Shared, captures: &[Value]) -> Result<(), Error> {
    let Value::Table(lua) = vm.get_global(b"lua")? else {
        unreachable!("song lua service table")
    };
    let function = vm.new_host_function_with_data(report, captures, store.clone())?;
    let root = vm.pin(function)?;
    let result = super::super::put(vm, lua, "ReportScriptError", function);
    vm.unpin(root)?;
    result?;
    let copy = vm.new_host_function_with_data(copy, captures, store.clone())?;
    let root = vm.pin(copy)?;
    let result = vm.set_global(b"DeepCopy", copy);
    vm.unpin(root)?;
    result
}

pub(super) fn initialize_base(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
) -> Result<(), Error> {
    let mut base = store.lock().unwrap().actors[&actor].base_transform;
    for field in crate::base_transform::BaseField::ALL {
        let value = get(call, actor, field.state_key().as_bytes())?;
        if value != Value::Nil {
            // Keep construction's existing lazy validation of malformed raw
            // fields. Primitive conversion uses the original captured builtin.
            if let Some(Value::Number(value)) = call.call(helper(call, 6)?, &[value])?.first() {
                base.set(field, *value as f32);
            }
        }
    }
    store
        .lock()
        .unwrap()
        .actors
        .get_mut(&actor)
        .unwrap()
        .base_transform = base;
    Ok(())
}

/// A copied live definition exports ordinary raw pose fields. A new native
/// actor starts at those destinations with its own empty tween queue.
pub(super) fn initialize_pose(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
) -> Result<(), Error> {
    let (mut pose, mut render, base) = {
        let guard = store.lock().unwrap();
        let actor = &guard.actors[&actor];
        (actor.current, actor.state.render, actor.base_transform)
    };
    macro_rules! scalars {
        ($($field:ident => $key:literal),* $(,)?) => {$(
            if let Some(value)=raw_number(call,actor,$key)? {pose.$field=value;}
        )*};
    }
    scalars! {
        aux=>"__songlua_state_aux",x=>"__songlua_state_x",y=>"__songlua_state_y",z=>"__songlua_state_z",
        zoom=>"__songlua_state_zoom",zoom_x=>"__songlua_state_zoom_x",zoom_y=>"__songlua_state_zoom_y",zoom_z=>"__songlua_state_zoom_z",
        rot_x_deg=>"__songlua_state_rot_x_deg",rot_y_deg=>"__songlua_state_rot_y_deg",rot_z_deg=>"__songlua_state_rot_z_deg",
        skew_x=>"__songlua_state_skew_x",skew_y=>"__songlua_state_skew_y",
        cropleft=>"__songlua_state_cropleft",cropright=>"__songlua_state_cropright",croptop=>"__songlua_state_croptop",cropbottom=>"__songlua_state_cropbottom",
        fadeleft=>"__songlua_state_fadeleft",faderight=>"__songlua_state_faderight",fadetop=>"__songlua_state_fadetop",fadebottom=>"__songlua_state_fadebottom",
    }
    for (key, output) in [
        ("__songlua_state_diffuse", &mut pose.diffuse),
        ("__songlua_state_glow", &mut pose.glow),
        ("__songlua_state_quaternion", &mut pose.quaternion),
    ] {
        let value = get(call, actor, key.as_bytes())?;
        if let Some(value) = components::<4>(call, value)? {
            *output = value;
        }
    }
    let value = get(call, actor, b"__songlua_state_vertex_colors")?;
    if let Value::Table(table) = value {
        let colors = call.with_roots(&[value], |call| {
            let mut colors = [[0.; 4]; 4];
            for (i, color) in colors.iter_mut().enumerate() {
                let value = call.raw_table_get(table, Value::Number((i + 1) as f64))?;
                let Some(value) = components::<4>(call, value)? else {
                    return Ok(None);
                };
                *color = value;
            }
            Ok(Some(colors))
        })?;
        if let Some(colors) = colors {
            pose.vertex_colors = Some(colors);
        }
    }
    use crate::base_transform::BaseField;
    for (field, out) in [
        (BaseField::Zoom, &mut render.basezoom),
        (BaseField::ZoomX, &mut render.basezoom_x),
        (BaseField::ZoomY, &mut render.basezoom_y),
        (BaseField::ZoomZ, &mut render.basezoom_z),
        (BaseField::HAlign, &mut render.halign),
        (BaseField::VAlign, &mut render.valign),
    ] {
        let value = base.get(field).or_else(|| {
            matches!(
                field,
                BaseField::ZoomX | BaseField::ZoomY | BaseField::ZoomZ
            )
            .then(|| base.get(BaseField::Zoom))
            .flatten()
        });
        if let Some(value) = value {
            *out = value;
        }
    }
    macro_rules! flags {
        ($($field:ident),* $(,)?) => {$(
            let value=get(call,actor,concat!("__songlua_state_",stringify!($field)).as_bytes())?;
            if value!=Value::Nil {render.$field=value.is_truthy();}
        )*};
    }
    flags! {draw_by_z_position,mask_source,mask_dest,depth_test,depth_write,depth_test_on_fail,texture_filtering,texture_wrapping}
    let mut guard = store.lock().unwrap();
    let actor = guard.actors.get_mut(&actor).unwrap();
    actor.current = pose;
    actor.state.rotation = [pose.rot_x_deg, pose.rot_y_deg, pose.rot_z_deg];
    pose.write_to(&mut render);
    actor.state.render = render;
    Ok(())
}

fn raw_number(call: &mut HostCall<'_>, actor: TableId, key: &str) -> Result<Option<f32>, Error> {
    let value = get(call, actor, key.as_bytes())?;
    number(call, value)
}
fn number(call: &mut HostCall<'_>, value: Value) -> Result<Option<f32>, Error> {
    if value == Value::Nil {
        return Ok(None);
    }
    Ok(match call.call(helper(call, 6)?, &[value])?.first() {
        Some(Value::Number(value)) => Some(*value as f32),
        _ => None,
    })
}
fn components<const N: usize>(
    call: &mut HostCall<'_>,
    value: Value,
) -> Result<Option<[f32; N]>, Error> {
    let Value::Table(table) = value else {
        return Ok(None);
    };
    call.with_roots(&[value], |call| {
        let mut out = [0.; N];
        for (i, component) in out.iter_mut().enumerate() {
            let value = call.raw_table_get(table, Value::Number((i + 1) as f64))?;
            let Some(value) = number(call, value)? else {
                return Ok(None);
            };
            *component = value;
        }
        Ok(Some(out))
    })
}

pub(super) fn update_base(
    actor: &mut Actor,
    target: crate::SongLuaOverlayUpdateTarget,
    value: &crate::SongLuaOverlayUpdateValue,
) {
    if let crate::SongLuaOverlayUpdateValue::F32(value) = value
        && let Some(field) = crate::base_transform::BaseField::from_key(
            crate::frame_capture::overlay_getter_scalar_key(target),
        )
    {
        actor.base_transform.set(field, *value);
    }
}

pub(super) fn raw_flag(
    call: &mut HostCall<'_>,
    store: &Shared,
    actor: TableId,
    key: &str,
    value: bool,
) -> Result<(), Error> {
    let owned = {
        let guard = store.lock().unwrap();
        guard.playback
            && guard.clock.live.load(std::sync::atomic::Ordering::Relaxed)
            && guard.actors.get(&actor).is_some_and(|a| !a.raw_live_epoch)
    };
    if !owned {
        sprites::set(call, actor, key, Value::Boolean(value))?;
    }
    Ok(())
}

fn copy(call: &mut HostCall<'_>) -> Result<(), Error> {
    let store = call.function_data::<Shared>().unwrap().clone();
    let value = arg(call, 0);
    let value = super::super::managers::compat::clone_value(call, value, &|call, source, into| {
        copy_pose(call, &store, source, into)
    })?;
    call.push(value)
}

fn copy_pose(
    call: &mut HostCall<'_>,
    store: &Shared,
    source: TableId,
    into: TableId,
) -> Result<(), Error> {
    // Clone native values and release Store before any allocation or VM entry.
    // Outgoing actors no longer have a native replay owner in the new epoch.
    let state = {
        let guard = store.lock().unwrap();
        let owned = guard.playback && guard.clock.live.load(std::sync::atomic::Ordering::Relaxed);
        guard
            .actors
            .get(&source)
            .filter(|a| !a.raw_live_epoch)
            .map(|actor| {
                let mut current = actor.current;
                if !owned && let Some([x, y, z, aux]) = actor.preparation_getters {
                    current.x = x;
                    current.y = y;
                    current.z = z;
                    current.aux = aux;
                }
                (
                    current,
                    *actor.destination(),
                    actor.state.render,
                    actor.base_transform,
                    owned,
                    actor.pose_exported,
                )
            })
    };
    let Some((current, dest, render, base, owned, exported)) = state else {
        return Ok(());
    };
    // Before playback the native host serializes its replay getters after a
    // pose write. Untouched actors retain their ordinary raw-table shape.
    if owned || exported {
        for (key, color) in [
            ("__songlua_diffuse", dest.diffuse),
            ("__songlua_state_diffuse", dest.diffuse),
            ("__songlua_state_glow", dest.glow),
        ] {
            let color = vector(call, &color)?;
            put(call, into, key, color)?;
        }
        let colors = if let Some(colors) = dest.vertex_colors {
            let out = call.try_new_table()?;
            call.with_roots(&[Value::Table(out)], |call| {
                for (index, color) in colors.iter().enumerate() {
                    let color = vector(call, color)?;
                    call.with_roots(&[color], |call| {
                        call.raw_table_set(out, Value::Number((index + 1) as f64), color)
                    })?;
                }
                Ok(Value::Table(out))
            })?
        } else {
            Value::Nil
        };
        put(call, into, "__songlua_state_vertex_colors", colors)?;
        for (key, value) in [
            ("__songlua_state_aux", dest.aux),
            ("__songlua_state_x", dest.x),
            ("__songlua_state_y", dest.y),
            ("__songlua_state_z", dest.z),
            ("__songlua_state_zoom", dest.zoom),
            ("__songlua_state_zoom_x", dest.zoom_x),
            ("__songlua_state_zoom_y", dest.zoom_y),
            ("__songlua_state_zoom_z", dest.zoom_z),
            ("__songlua_state_skew_x", dest.skew_x),
            ("__songlua_state_skew_y", dest.skew_y),
            ("__songlua_state_cropleft", dest.cropleft),
            ("__songlua_state_cropright", dest.cropright),
            ("__songlua_state_croptop", dest.croptop),
            ("__songlua_state_cropbottom", dest.cropbottom),
            ("__songlua_state_fadeleft", dest.fadeleft),
            ("__songlua_state_faderight", dest.faderight),
            ("__songlua_state_fadetop", dest.fadetop),
            ("__songlua_state_fadebottom", dest.fadebottom),
            ("__songlua_aux", current.aux),
            ("__songlua_current_x", current.x),
            ("__songlua_current_y", current.y),
            ("__songlua_current_z", current.z),
        ] {
            put(call, into, key, Value::Number(value as f64))?;
        }
        if !owned {
            for (key, value) in [
                ("__songlua_state_rot_x_deg", dest.rot_x_deg),
                ("__songlua_state_rot_y_deg", dest.rot_y_deg),
                ("__songlua_state_rot_z_deg", dest.rot_z_deg),
            ] {
                put(call, into, key, Value::Number(value as f64))?;
            }
            let quaternion = vector(call, &dest.quaternion)?;
            put(call, into, "__songlua_state_quaternion", quaternion)?;
        }
    }
    if owned {
        let flags = crate::render_flags::RenderFlags::from(render);
        for flag in crate::render_flags::RenderFlag::ALL {
            put(
                call,
                into,
                flag.state_key(),
                Value::Boolean(flags.get(flag)),
            )?;
        }
    }
    for field in crate::base_transform::BaseField::ALL {
        if owned || base.get(field).is_some() {
            put(
                call,
                into,
                field.state_key(),
                base.get(field)
                    .map_or(Value::Nil, |v| Value::Number(v as f64)),
            )?;
        }
    }
    Ok(())
}

fn vector(call: &mut HostCall<'_>, values: &[f32]) -> Result<Value, Error> {
    let out = call.try_new_table()?;
    call.with_roots(&[Value::Table(out)], |call| {
        for (i, value) in values.iter().enumerate() {
            call.raw_table_set(
                out,
                Value::Number((i + 1) as f64),
                Value::Number(*value as f64),
            )?;
        }
        Ok(Value::Table(out))
    })
}
fn put(call: &mut HostCall<'_>, into: TableId, key: &str, value: Value) -> Result<(), Error> {
    call.with_roots(&[value], |call| sprites::set(call, into, key, value))
}

fn report(call: &mut HostCall<'_>) -> Result<(), Error> {
    let store = call.function_data::<Shared>().unwrap().clone();
    let index = usize::from(matches!(call.arguments().first(), Some(Value::Table(_))));
    let message = match call.arguments().get(index) {
        Some(Value::String(id)) => std::str::from_utf8(call.string_bytes(*id).unwrap())
            .ok()
            .map(str::to_owned),
        _ => None,
    }
    .ok_or_else(|| call.error(b"ReportScriptError: string expected"))?;
    framebuffers::report(call, &store, &message)
}

#[cfg(feature = "test-support")]
impl Actors {
    pub(crate) fn fixture_destination_vertices(&self, value: Value) -> Option<[[f32; 4]; 4]> {
        let Value::Table(id) = value else { return None };
        let store = self.store.lock().unwrap();
        store
            .actors
            .get(&id)
            .map(|actor| actor.destination().vertex_colors.unwrap_or([[1.; 4]; 4]))
    }
    pub(crate) fn fixture_live_color_storage(
        &self,
        value: Value,
    ) -> Option<(usize, [f32; 4], [f32; 4])> {
        let Value::Table(id) = value else { return None };
        let store = self.store.lock().unwrap();
        let actor = store.actors.get(&id)?;
        Some((
            &actor.current.diffuse as *const [f32; 4] as usize,
            actor.current.diffuse,
            actor.destination().diffuse,
        ))
    }
    pub(crate) fn fixture_copy_live_pose(
        &self,
        vm: &mut Vm,
        source: Value,
        into: Value,
    ) -> Result<(), Error> {
        let callback = vm.new_host_function_with_data(
            |call| {
                let store = call.function_data::<Shared>().unwrap().clone();
                let source = table(call, call.captures()[2])?;
                let into = table(call, call.captures()[3])?;
                copy_pose(call, &store, source, into)
            },
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                source,
                into,
            ],
            self.store.clone(),
        )?;
        vm.call(callback, &[]).map(|_| ())
    }
    pub(crate) fn fixture_export_live_state(
        &self,
        vm: &mut Vm,
        source: Value,
        mutable: bool,
    ) -> Result<deadlib_lua::Root, Error> {
        let callback = vm.new_host_function_with_data(
            |call| {
                let (store, mutable) = call.function_data::<(Shared, bool)>().unwrap().clone();
                let source = table(call, call.captures()[2])?;
                let saved = if mutable {
                    state_snapshot::raw_copy_mutable(call, source)?
                } else {
                    state_snapshot::raw_copy(call, source)?
                };
                call.with_roots(&[Value::Table(saved)], |call| {
                    copy_pose(call, &store, source, saved)?;
                    call.push(Value::Table(saved))
                })
            },
            &[
                Value::Table(self.registry),
                Value::Table(self.helpers),
                source,
            ],
            (self.store.clone(), mutable),
        )?;
        let value = vm
            .call(callback, &[])?
            .first()
            .copied()
            .unwrap_or(Value::Nil);
        vm.pin(value)
    }
    pub(crate) fn fixture_restore_live_state(
        &self,
        vm: &mut Vm,
        into: Value,
        saved: Value,
    ) -> Result<(), Error> {
        let callback = vm.new_host_function(
            |call| {
                let into = table(call, call.captures()[0])?;
                let saved = table(call, call.captures()[1])?;
                state_snapshot::restore_mutable(call, into, saved, true)
            },
            &[into, saved],
        )?;
        vm.call(callback, &[]).map(|_| ())
    }
}
