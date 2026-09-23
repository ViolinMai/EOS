extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use crate::syscall::{print_num, print_str};

pub struct PngImage {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u32>,
}

struct BitReader<'a> {
    data: &'a [u8],
    bit_pos: usize,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, bit_pos: 0 }
    }

    fn read_bits(&mut self, n: usize) -> Option<u32> {
        let mut res = 0u32;
        for i in 0..n {
            let byte_idx = self.bit_pos >> 3;
            let bit_idx = self.bit_pos & 7;
            if byte_idx >= self.data.len() {
                return None;
            }
            let bit = (self.data[byte_idx] >> bit_idx) & 1;
            res |= (bit as u32) << i;
            self.bit_pos += 1;
        }
        Some(res)
    }

    fn align_byte(&mut self) {
        let rem = self.bit_pos & 7;
        if rem != 0 {
            self.bit_pos += 8 - rem;
        }
    }
}

fn decompress_zlib(data: &[u8], expected_len: usize) -> Result<Vec<u8>, &'static str> {
    if data.len() < 6 {
        return Err("Zlib buffer too small\n");
    }
    let cmf = data[0];
    if (cmf & 0x0F) != 8 {
        return Err("Unsupported compression method\n");
    }

    let payload = &data[2..data.len() - 4];
    let mut reader = BitReader::new(payload);
    let mut out = Vec::with_capacity(expected_len);

    let mut is_final = false;
    while !is_final {
        let bfinal = match reader.read_bits(1) {
            Some(b) => b,
            None => return Err("Deflate EOF\n"),
        };
        is_final = bfinal == 1;
        let btype = match reader.read_bits(2) {
            Some(b) => b,
            None => return Err("Deflate EOF\n"),
        };

        match btype {
            0 => {
                reader.align_byte();
                let len = match reader.read_bits(16) {
                    Some(l) => l as usize,
                    None => return Err("EOF\n"),
                };
                let _nlen = reader.read_bits(16);
                for _ in 0..len {
                    if let Some(b) = reader.read_bits(8) {
                        out.push(b as u8);
                    } else {
                        return Err("EOF\n");
                    }
                }
            }
            1 => {
                decode_fixed_huffman(&mut reader, &mut out)?;
            }
            2 => {
                decode_dynamic_huffman(&mut reader, &mut out)?;
            }
            _ => return Err("Invalid Deflate block type\n"),
        }
    }

    Ok(out)
}

struct FastHuffman {
    counts: [u16; 16],
    symbols: Vec<u16>,
}

impl FastHuffman {
    fn build(lengths: &[u8]) -> Self {
        let mut counts = [0u16; 16];
        for i in 0..lengths.len() {
            let len = lengths[i];
            if len > 0 && (len as usize) < 16 {
                counts[len as usize] += 1;
            }
        }

        let mut symbols = Vec::with_capacity(lengths.len());
        for len in 1..16 {
            for i in 0..lengths.len() {
                if lengths[i] == len as u8 {
                    symbols.push(i as u16);
                }
            }
        }

        Self { counts, symbols }
    }

    fn decode(&self, reader: &mut BitReader) -> Option<u16> {
        let mut code = 0u16;
        let mut first = 0u16;
        let mut index = 0usize;

        for len in 1..16 {
            let bit = reader.read_bits(1)? as u16;
            code |= bit << (len - 1);
            let count = self.counts[len];

            let mut rev_code = 0u16;
            for b in 0..len {
                if (code & (1 << b)) != 0 {
                    rev_code |= 1 << (len - 1 - b);
                }
            }

            if rev_code < first + count {
                return Some(self.symbols[index + (rev_code - first) as usize]);
            }
            index += count as usize;
            first = (first + count) << 1;
        }
        None
    }
}

const LENGTH_BASES: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258
];
const LENGTH_EXTRA_BITS: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0
];

const DIST_BASES: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577
];
const DIST_EXTRA_BITS: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13
];

const CL_ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

fn decode_fixed_huffman(reader: &mut BitReader, out: &mut Vec<u8>) -> Result<(), &'static str> {
    let mut lengths = [8u8; 288];
    for i in 144..256 { lengths[i] = 9; }
    for i in 256..280 { lengths[i] = 7; }
    for i in 280..288 { lengths[i] = 8; }
    let lit = FastHuffman::build(&lengths);
    let dist = FastHuffman::build(&[5u8; 32]);
    decode_huffman_stream(reader, out, &lit, &dist)
}

fn decode_dynamic_huffman(reader: &mut BitReader, out: &mut Vec<u8>) -> Result<(), &'static str> {
    let hlit = match reader.read_bits(5) {
        Some(v) => (v + 257) as usize,
        None => return Err("EOF\n"),
    };
    let hdist = match reader.read_bits(5) {
        Some(v) => (v + 1) as usize,
        None => return Err("EOF\n"),
    };
    let hclen = match reader.read_bits(4) {
        Some(v) => (v + 4) as usize,
        None => return Err("EOF\n"),
    };

    let mut cl_lengths = [0u8; 19];
    for i in 0..hclen {
        if let Some(b) = reader.read_bits(3) {
            cl_lengths[CL_ORDER[i]] = b as u8;
        } else {
            return Err("EOF\n");
        }
    }
    let cl_table = FastHuffman::build(&cl_lengths);

    let mut all_lengths = Vec::with_capacity(hlit + hdist);
    while all_lengths.len() < (hlit + hdist) {
        let sym = match cl_table.decode(reader) {
            Some(s) => s,
            None => return Err("CL Huffman fail\n"),
        };
        if sym <= 15 {
            all_lengths.push(sym as u8);
        } else if sym == 16 {
            let prev = if let Some(&p) = all_lengths.last() { p } else { return Err("CL error\n"); };
            let rep = match reader.read_bits(2) {
                Some(r) => r + 3,
                None => return Err("EOF\n"),
            };
            for _ in 0..rep { all_lengths.push(prev); }
        } else if sym == 17 {
            let rep = match reader.read_bits(3) {
                Some(r) => r + 3,
                None => return Err("EOF\n"),
            };
            for _ in 0..rep { all_lengths.push(0); }
        } else if sym == 18 {
            let rep = match reader.read_bits(7) {
                Some(r) => r + 11,
                None => return Err("EOF\n"),
            };
            for _ in 0..rep { all_lengths.push(0); }
        }
    }

    let lit = FastHuffman::build(&all_lengths[..hlit]);
    let dist = FastHuffman::build(&all_lengths[hlit..]);
    decode_huffman_stream(reader, out, &lit, &dist)
}

fn decode_huffman_stream(
    reader: &mut BitReader,
    out: &mut Vec<u8>,
    lit_table: &FastHuffman,
    dist_table: &FastHuffman,
) -> Result<(), &'static str> {
    loop {
        let sym = match lit_table.decode(reader) {
            Some(s) => s,
            None => return Err("Lit decode err\n"),
        };
        if sym < 256 {
            out.push(sym as u8);
        } else if sym == 256 {
            break;
        } else {
            let len_idx = (sym - 257) as usize;
            if len_idx >= 29 { return Err("Invalid length symbol\n"); }
            let len_extra = match reader.read_bits(LENGTH_EXTRA_BITS[len_idx] as usize) {
                Some(e) => e,
                None => return Err("EOF\n"),
            };
            let length = LENGTH_BASES[len_idx] + len_extra as u16;

            let dist_sym = match dist_table.decode(reader) {
                Some(s) => s as usize,
                None => return Err("Dist decode err\n"),
            };
            if dist_sym >= 30 { return Err("Invalid dist symbol\n"); }
            let dist_extra = match reader.read_bits(DIST_EXTRA_BITS[dist_sym] as usize) {
                Some(e) => e,
                None => return Err("EOF\n"),
            };
            let distance = (DIST_BASES[dist_sym] + dist_extra as u16) as usize;

            if distance > out.len() {
                return Err("Distance exceeds output buffer\n");
            }

            let start = out.len() - distance;
            for i in 0..length as usize {
                let b = out[start + i];
                out.push(b);
            }
        }
    }
    Ok(())
}

fn paeth(a: u8, b: u8, c: u8) -> u8 {
    let a_i = a as i32;
    let b_i = b as i32;
    let c_i = c as i32;
    let p = a_i + b_i - c_i;
    let pa = (p - a_i).abs();
    let pb = (p - b_i).abs();
    let pc = (p - c_i).abs();
    if pa <= pb && pa <= pc {
        a
    } else if pb <= pc {
        b
    } else {
        c
    }
}

pub fn decode_png(data: &[u8]) -> Result<PngImage, &'static str> {
    if data.len() < 33 {
        return Err("File too small for PNG header\n");
    }

    // 1. تحقق مباشر وصارم من بداية الملف عند البايت 0 تماماً
    const PNG_SIG: [u8; 8] = [0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
    if &data[0..8] != &PNG_SIG {
        return Err("Invalid PNG signature at offset 0\n");
    }

    // 2. التحقق من IHDR (من البايت 12 إلى 16)
    if &data[12..16] != b"IHDR" {
        return Err("First chunk is not IHDR\n");
    }

    // 3. قراءة الحقول بالـ Offsets الثابتة المطلقة
    let width = u32::from_be_bytes([data[16], data[17], data[18], data[19]]) as usize;
    let height = u32::from_be_bytes([data[20], data[21], data[22], data[23]]) as usize;
    let bit_depth = data[24];
    let color_type = data[25];

    print_str("[User App] Exact Header: ");
    print_num(width);
    print_str("x");
    print_num(height);
    print_str(" | Depth=");
    print_num(bit_depth as usize);
    print_str(" | ColorType=");
    print_num(color_type as usize);
    print_str("\n");

    if bit_depth != 8 {
        return Err("Only 8-bit depth PNG is supported\n");
    }

    let bpp = match color_type {
        2 => 3, // Truecolor RGB
        6 => 4, // Truecolor with Alpha RGBA
        _ => return Err("Only Truecolor RGB(2) or RGBA(6) is supported\n"),
    };

    // 4. تجميع قطع IDAT من بعد نهاية IHDR (Byte 33: 8 sig + 4 len + 4 type + 13 data + 4 crc)
    let mut offset = 33;
    let mut idat_data = Vec::with_capacity(data.len());
    let mut idat_count = 0usize;

    while offset + 8 <= data.len() {
        let length = u32::from_be_bytes([data[offset], data[offset + 1], data[offset + 2], data[offset + 3]]) as usize;
        let chunk_type = &data[offset + 4..offset + 8];
        let chunk_data_offset = offset + 8;

        if chunk_data_offset + length > data.len() {
            break;
        }

        if chunk_type == b"IDAT" {
            let slice = &data[chunk_data_offset..chunk_data_offset + length];
            for i in 0..length {
                idat_data.push(slice[i]);
            }
            idat_count += 1;
        } else if chunk_type == b"IEND" {
            break;
        }

        offset = chunk_data_offset + length + 4; // تخطي Data و CRC
    }

    if idat_data.is_empty() {
        return Err("No IDAT chunk found\n");
    }

    print_str("[User App] IDAT Chunks Found: ");
    print_num(idat_count);
    print_str(" (Total compressed size: ");
    print_num(idat_data.len());
    print_str(" bytes)\n");

    let stride = width * bpp;
    let expected_uncompressed = (stride + 1) * height;

    print_str("[User App] Starting zlib decompression...\n");
    let raw_decompressed = decompress_zlib(&idat_data, expected_uncompressed)?;
    if raw_decompressed.len() < expected_uncompressed {
        return Err("Decompressed IDAT shorter than expected\n");
    }

    print_str("[User App] Decompression complete! Rendering pixels...\n");

    let mut pixels = Vec::with_capacity(width * height);
    let mut prev_row = vec![0u8; stride];
    let mut in_cursor = 0usize;

    for _ in 0..height {
        let filter = raw_decompressed[in_cursor];
        in_cursor += 1;
        let mut curr_row = vec![0u8; stride];

        for x in 0..stride {
            let raw = raw_decompressed[in_cursor + x];
            let left = if x >= bpp { curr_row[x - bpp] } else { 0 };
            let up = prev_row[x];
            let up_left = if x >= bpp { prev_row[x - bpp] } else { 0 };

            let filtered = match filter {
                0 => raw,
                1 => raw.wrapping_add(left),
                2 => raw.wrapping_add(up),
                3 => raw.wrapping_add(((left as u16 + up as u16) >> 1) as u8),
                4 => raw.wrapping_add(paeth(left, up, up_left)),
                _ => return Err("Unknown PNG scanline filter\n"),
            };
            curr_row[x] = filtered;
        }

        for x in 0..width {
            let offset = x * bpp;
            let r = curr_row[offset] as u32;
            let g = curr_row[offset + 1] as u32;
            let b = curr_row[offset + 2] as u32;
            let a = if bpp == 4 { curr_row[offset + 3] as u32 } else { 255u32 };
            pixels.push((a << 24) | (r << 16) | (g << 8) | b);
        }

        prev_row = curr_row;
        in_cursor += stride;
    }

    Ok(PngImage { width, height, pixels })
}
