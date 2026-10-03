// Frozen from cafbd9e2a (0.5.1707).
use super::*;

pub(super) fn queue_install_snapshot(
    runtime: &mut RuntimeState,
    pack: &PackInfo,
) -> Result<(), String> {
    let mut snapshot = (*runtime.snapshot).clone();
    if let Some(install) = snapshot
        .installs
        .iter_mut()
        .find(|install| install.pack_id == pack.id)
    {
        match install.phase {
            InstallPhase::Queued | InstallPhase::Downloading | InstallPhase::Extracting => {
                return Err(format!("'{}' is already queued.", pack.name));
            }
            InstallPhase::Installed => {
                return Err(format!(
                    "'{}' was already installed this session.",
                    pack.name
                ));
            }
            InstallPhase::Error => {
                *install = queued_install(pack);
                runtime.snapshot = Arc::new(snapshot);
                return Ok(());
            }
        }
    }
    if snapshot.installs.len() == MAX_INSTALLS {
        let terminal = snapshot.installs.iter().position(|install| {
            matches!(install.phase, InstallPhase::Installed | InstallPhase::Error)
        });
        let Some(index) = terminal else {
            return Err("Too many pack installs are active.".to_string());
        };
        let evicted = snapshot.installs.remove(index);
        log::debug!(
            "Evicted terminal StepManiaOnline install history for pack {}.",
            evicted.pack_id
        );
    }
    snapshot.installs.push(queued_install(pack));
    runtime.snapshot = Arc::new(snapshot);
    Ok(())
}
