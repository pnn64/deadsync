use alloc::vec::Vec;

/// SEI message (H.264 spec section 7.3.2.3).
#[derive(Debug)]
pub struct SeiMessage {
    pub payload_type: u32,
    pub payload: SeiPayload,
}

#[derive(Debug)]
pub enum SeiPayload {
    UserDataUnregistered { uuid: [u8; 16], data: Vec<u8> },
    Unknown { data: Vec<u8> },
}

/// Parse all SEI messages from RBSP data (NAL header byte already stripped).
pub fn parse_sei(rbsp: &[u8]) -> Result<Vec<SeiMessage>, &'static str> {
    let mut messages = Vec::new();
    let mut offset = 0;

    while offset < rbsp.len() {
        // Check for RBSP trailing bits (byte with MSB set and rest zero, e.g. 0x80)
        if rbsp[offset] == 0x80 && rbsp[offset..].iter().all(|&b| b == 0x80 || b == 0x00) {
            break;
        }

        // Read payloadType: sum of 0xFF bytes, plus final byte < 0xFF
        let mut payload_type: u32 = 0;
        while offset < rbsp.len() && rbsp[offset] == 0xFF {
            payload_type += 255;
            offset += 1;
        }
        if offset >= rbsp.len() {
            return Err("unexpected end of SEI payload type");
        }
        payload_type += rbsp[offset] as u32;
        offset += 1;

        // Read payloadSize: same encoding
        let mut payload_size: u32 = 0;
        while offset < rbsp.len() && rbsp[offset] == 0xFF {
            payload_size += 255;
            offset += 1;
        }
        if offset >= rbsp.len() {
            return Err("unexpected end of SEI payload size");
        }
        payload_size += rbsp[offset] as u32;
        offset += 1;

        let size = payload_size as usize;
        if offset + size > rbsp.len() {
            return Err("SEI payload size exceeds RBSP length");
        }

        let payload_data = &rbsp[offset..offset + size];
        offset += size;

        let payload = match payload_type {
            5 => parse_user_data_unregistered(payload_data)?,
            _ => SeiPayload::Unknown {
                data: payload_data.to_vec(),
            },
        };

        messages.push(SeiMessage {
            payload_type,
            payload,
        });
    }

    Ok(messages)
}

fn parse_user_data_unregistered(data: &[u8]) -> Result<SeiPayload, &'static str> {
    if data.len() < 16 {
        return Err("user_data_unregistered payload too short for UUID");
    }
    let mut uuid = [0u8; 16];
    uuid.copy_from_slice(&data[..16]);
    Ok(SeiPayload::UserDataUnregistered {
        uuid,
        data: data[16..].to_vec(),
    })
}
