#[cfg(all(not(target_pointer_width = "32"), not(target_vendor = "win7")))]
pub mod vulkan;
