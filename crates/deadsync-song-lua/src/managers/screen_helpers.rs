//! Existing screen routing, background fit and Simply Love utility globals.
use super::catalog::{arg, text};
use super::fields::{object, set};
use super::*;

pub(super) fn install(vm: &mut Vm) -> Result<(), Error> {
    let globals = vm.get_global(b"_G")?;
    let setup = vm.new_host_function(initialize, &[globals])?;
    vm.call(setup, &[])?;
    for (source, name) in [
        (
            include_bytes!("background_fit.lua").as_slice(),
            b"background fit helpers".as_slice(),
        ),
        (
            include_bytes!("sl_utilities.lua").as_slice(),
            b"@theme:/Scripts/06 SL-Utilities.lua".as_slice(),
        ),
    ] {
        vm.execute(&deadlib_lua::compile_named(source, name)?)?;
    }
    Ok(())
}
fn initialize(call: &mut HostCall<'_>) -> Result<(), Error> {
    let globals = table(call, call.captures()[0])?;
    let screen_string = call.new_host_function(screen_string, &[])?;
    set(globals, "ScreenString", screen_string, call)?;
    let credits = object(call, |call, owner| {
        super::scores::method(call, owner, "GetCreditsMessage", "Free Play")
    })?;
    set(globals, "ScreenSystemLayerHelpers", credits, call)?;
    let branch = object(call, |call, owner| {
        for (name, screen) in [
            ("AfterScreenRankingDouble", "ScreenRainbow"),
            ("TitleMenu", "ScreenTitleMenu"),
            ("AfterInit", "ScreenTitleMenu"),
            ("AllowScreenSelectProfile", "ScreenSelectProfile"),
            ("AfterSelectProfile", "ScreenSelectColor"),
            ("AllowScreenSelectColor", "ScreenSelectColor"),
            ("AfterScreenSelectColor", "ScreenSelectStyle"),
            ("AllowScreenSelectPlayMode", "ScreenSelectPlayMode"),
            ("AllowScreenSelectPlayMode2", "ScreenProfileLoad"),
            ("AfterSelectPlayMode", "ScreenSelectMusic"),
            ("AfterEvaluationStage", "ScreenProfileSave"),
            ("AfterGameplay", "ScreenEvaluationStage"),
            ("AfterHeartEntry", "ScreenEvaluationStage"),
            ("AfterSelectMusic", "ScreenGameplay"),
            ("SSMCancel", "ScreenTitleMenu"),
            ("AllowScreenNameEntry", "ScreenNameEntryTraditional"),
            ("AllowScreenEvalSummary", "ScreenEvaluationSummary"),
            ("AfterProfileSave", "ScreenSelectMusic"),
            ("AfterProfileSaveSummary", "ScreenGameOver"),
            ("GameplayScreen", "ScreenGameplay"),
        ] {
            super::scores::method(call, owner, name, screen)?;
        }
        Ok(())
    })?;
    set(globals, "Branch", branch, call)?;
    super::scores::method(call, globals, "SelectMusicOrCourse", "ScreenSelectMusic")
}
fn screen_string(call: &mut HostCall<'_>) -> Result<(), Error> {
    let name = text(call, arg(call, 0)).unwrap_or_default();
    let result = call.try_intern(crate::theme_string("", &name).as_bytes())?;
    call.push(result)
}
