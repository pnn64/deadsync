// Frozen from bb638e5f9 for output/capacity and paired CPU checks.
use super::GamepadCodeBinding;
pub fn gamepad_code_binding_to_token(binding: GamepadCodeBinding) -> String {
    const BASE_LEN: usize = "PadCode[0x00000000]".len();
    const UUID_LEN: usize = 33;

    let device_len = binding.device.map_or(0, |mut device| {
        let mut digits = 1;
        while device >= 10 {
            device /= 10;
            digits += 1;
        }
        digits + 1
    });
    let capacity = BASE_LEN + device_len + if binding.uuid.is_some() { UUID_LEN } else { 0 };
    let mut s = String::with_capacity(capacity);
    write_gamepad_code_binding_token(&mut s, binding);
    s
}
pub fn write_gamepad_code_binding_token(s: &mut String, binding: GamepadCodeBinding) {
    use std::fmt::Write;
    let _ = write!(s, "PadCode[0x{:08X}]", binding.code_u32);
    if let Some(device) = binding.device {
        let _ = write!(s, "@{device}");
    }
    if let Some(uuid) = binding.uuid {
        const HEX: &[u8; 16] = b"0123456789ABCDEF";
        s.push('#');
        for &byte in &uuid {
            s.push(char::from(HEX[usize::from(byte >> 4)]));
            s.push(char::from(HEX[usize::from(byte & 15)]));
        }
    }
}
