// Frozen from d49e2923567b5f1bc32e599b0bceda123c67ea56.
use super::*;
#[must_use]
pub fn is_mac_resource_fork_original(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with("._"))
}
