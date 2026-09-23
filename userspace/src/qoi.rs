// مفكك تشفير QOI خفيف ومستقل تماماً بدون float وبدون std
pub struct QoiHeader {
    pub width: usize,
    pub height: usize,
    pub channels: u8,
}

pub fn decode_qoi(data: &[u8], out_pixels: &mut [u32]) -> Result<QoiHeader, &'static str> {
    if data.len() < 14 {
        return Err("QOI data too short");
    }

    if &data[0..4] != b"qoif" {
        return Err("Invalid QOI magic bytes");
    }

    let width = ((data[4] as usize) << 24)
        | ((data[5] as usize) << 16)
        | ((data[6] as usize) << 8)
        | (data[7] as usize);

    let height = ((data[8] as usize) << 24)
        | ((data[9] as usize) << 16)
        | ((data[10] as usize) << 8)
        | (data[11] as usize);

    let channels = data[12];

    if out_pixels.len() < width * height {
        return Err("Output buffer too small");
    }

    let mut index = [[0u8; 4]; 64];
    let mut px = [0u8, 0, 0, 255]; // RGBA
    let mut p_idx = 0usize;
    let mut in_pos = 14usize;
    let total_pixels = width * height;

    while in_pos < data.len() && p_idx < total_pixels {
        let b1 = data[in_pos];
        in_pos += 1;

        if b1 == 0b11111110 {
            // QOI_OP_RGB
            if in_pos + 3 > data.len() { break; }
            px[0] = data[in_pos];
            px[1] = data[in_pos + 1];
            px[2] = data[in_pos + 2];
            in_pos += 3;
        } else if b1 == 0b11111111 {
            // QOI_OP_RGBA
            if in_pos + 4 > data.len() { break; }
            px[0] = data[in_pos];
            px[1] = data[in_pos + 1];
            px[2] = data[in_pos + 2];
            px[3] = data[in_pos + 3];
            in_pos += 4;
        } else {
            let tag = b1 & 0b11000000;
            if tag == 0b00000000 {
                // QOI_OP_INDEX
                let idx = (b1 & 0x3F) as usize;
                px = index[idx];
            } else if tag == 0b01000000 {
                // QOI_OP_DIFF
                let dr = ((b1 >> 4) & 0x03).wrapping_sub(2);
                let dg = ((b1 >> 2) & 0x03).wrapping_sub(2);
                let db = (b1 & 0x03).wrapping_sub(2);
                px[0] = px[0].wrapping_add(dr);
                px[1] = px[1].wrapping_add(dg);
                px[2] = px[2].wrapping_add(db);
            } else if tag == 0b10000000 {
                // QOI_OP_LUMA
                if in_pos >= data.len() { break; }
                let b2 = data[in_pos];
                in_pos += 1;
                let dg = (b1 & 0x3F).wrapping_sub(32);
                let dr = dg.wrapping_add(((b2 >> 4) & 0x0F).wrapping_sub(8));
                let db = dg.wrapping_add((b2 & 0x0F).wrapping_sub(8));
                px[0] = px[0].wrapping_add(dr);
                px[1] = px[1].wrapping_add(dg);
                px[2] = px[2].wrapping_add(db);
            } else {
                // QOI_OP_RUN
                let run = ((b1 & 0x3F) + 1) as usize;
                for _ in 0..run {
                    if p_idx < total_pixels {
                        // حزم إلى 0xAARRGGBB
                        out_pixels[p_idx] = ((px[3] as u32) << 24)
                            | ((px[0] as u32) << 16)
                            | ((px[1] as u32) << 8)
                            | (px[2] as u32);
                        p_idx += 1;
                    }
                }
                continue;
            }
        }

        // تحديث جدول الـ Hash
        let hash = (px[0] as usize * 3 + px[1] as usize * 5 + px[2] as usize * 7 + px[3] as usize * 11) % 64;
        index[hash] = px;

        out_pixels[p_idx] = ((px[3] as u32) << 24)
            | ((px[0] as u32) << 16)
            | ((px[1] as u32) << 8)
            | (px[2] as u32);
        p_idx += 1;
    }

    Ok(QoiHeader { width, height, channels })
}
