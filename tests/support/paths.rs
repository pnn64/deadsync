use std::path::Path;

pub fn init() {
    static INIT: std::sync::Once = std::sync::Once::new();
    INIT.call_once(|| {
        let bundle = Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf();
        let data =
            std::env::temp_dir().join(format!("deadsync-integration-paths-{}", std::process::id()));
        let dirs = deadsync_config::dirs::AppDirs {
            cache_dir: data.join("cache"),
            data_dir: data,
            exe_dir: bundle,
            portable: false,
        };
        dirs.ensure_dirs_exist();
        deadsync_shell::app::init_paths(&dirs).expect("initialize isolated test services");
    });
}
