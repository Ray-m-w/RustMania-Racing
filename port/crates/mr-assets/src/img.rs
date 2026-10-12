//! Minimal PNG writer (stored deflate blocks, no compression) for debug
//! exports, so no image crate is needed.

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for i in 0..256u32 {
        let mut c = i;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xEDB88320 ^ (c >> 1) } else { c >> 1 };
        }
        table[i as usize] = c;
    }
    let mut c = 0xFFFF_FFFFu32;
    for &b in data {
        c = table[((c ^ b as u32) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut c = kind.to_vec();
    c.extend_from_slice(data);
    out.extend_from_slice(&c);
    out.extend_from_slice(&crc32(&c).to_be_bytes());
}

/// Encode RGBA8 pixels (w*h*4 bytes) as a PNG file.
pub fn png_rgba(w: u32, h: u32, rgba: &[u8]) -> Vec<u8> {
    assert_eq!(rgba.len(), (w * h * 4) as usize);
    let mut raw = Vec::with_capacity(((w * 4 + 1) * h) as usize);
    for y in 0..h as usize {
        raw.push(0);
        raw.extend_from_slice(&rgba[y * w as usize * 4..(y + 1) * w as usize * 4]);
    }
    let mut z = vec![0x78, 0x01];
    let mut a: u32 = 1;
    let mut b: u32 = 0;
    for &x in &raw {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    let mut i = 0;
    while i < raw.len() {
        let n = (raw.len() - i).min(65535);
        z.push(if i + n == raw.len() { 1 } else { 0 });
        z.extend_from_slice(&(n as u16).to_le_bytes());
        z.extend_from_slice(&(!(n as u16)).to_le_bytes());
        z.extend_from_slice(&raw[i..i + n]);
        i += n;
    }
    z.extend_from_slice(&((b << 16) | a).to_be_bytes());
    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}
