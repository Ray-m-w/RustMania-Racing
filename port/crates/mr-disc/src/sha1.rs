//! SHA-1 (FIPS 180-4), std only: the disc check compares the track file
//! with the Redump record (BINLIST.toml). Not for security.

pub struct Sha1 {
    h: [u32; 5],
    buf: [u8; 64],
    n: usize,
    len: u64,
}

impl Default for Sha1 {
    fn default() -> Self {
        Sha1 { h: [0x67452301, 0xEFCDAB89, 0x98BADCFE, 0x10325476, 0xC3D2E1F0], buf: [0; 64], n: 0, len: 0 }
    }
}

impl Sha1 {
    pub fn update(&mut self, mut data: &[u8]) {
        self.len += data.len() as u64;
        if self.n > 0 {
            let k = (64 - self.n).min(data.len());
            self.buf[self.n..self.n + k].copy_from_slice(&data[..k]);
            self.n += k;
            data = &data[k..];
            if self.n < 64 {
                return;
            }
            let b = self.buf;
            self.block(&b);
            self.n = 0;
        }
        let mut chunks = data.chunks_exact(64);
        for c in &mut chunks {
            self.block(c.try_into().unwrap());
        }
        let r = chunks.remainder();
        self.buf[..r.len()].copy_from_slice(r);
        self.n = r.len();
    }

    pub fn finish(mut self) -> [u8; 20] {
        let bits = self.len.wrapping_mul(8);
        self.update(&[0x80]);
        while self.n != 56 {
            self.update(&[0]);
        }
        self.update(&bits.to_be_bytes());
        let mut out = [0; 20];
        for (o, h) in out.chunks_exact_mut(4).zip(self.h) {
            o.copy_from_slice(&h.to_be_bytes());
        }
        out
    }

    fn block(&mut self, b: &[u8; 64]) {
        let mut w = [0u32; 80];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([b[4 * i], b[4 * i + 1], b[4 * i + 2], b[4 * i + 3]]);
        }
        for i in 16..80 {
            w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
        }
        let [mut a, mut bb, mut c, mut d, mut e] = self.h;
        for (i, wi) in w.iter().enumerate() {
            let (f, k) = match i {
                0..=19 => ((bb & c) | (!bb & d), 0x5A827999),
                20..=39 => (bb ^ c ^ d, 0x6ED9EBA1),
                40..=59 => ((bb & c) | (bb & d) | (c & d), 0x8F1BBCDC),
                _ => (bb ^ c ^ d, 0xCA62C1D6),
            };
            let t = a.rotate_left(5).wrapping_add(f).wrapping_add(e).wrapping_add(k).wrapping_add(*wi);
            e = d;
            d = c;
            c = bb.rotate_left(30);
            bb = a;
            a = t;
        }
        for (h, v) in self.h.iter_mut().zip([a, bb, c, d, e]) {
            *h = h.wrapping_add(v);
        }
    }
}

/// Lower-case hex of a file's SHA-1.
pub fn file_hex(path: &std::path::Path) -> std::io::Result<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut s = Sha1::default();
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        s.update(&buf[..n]);
    }
    Ok(hex(&s.finish()))
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(d: &[u8]) -> String {
        let mut s = Sha1::default();
        s.update(d);
        hex(&s.finish())
    }

    #[test]
    fn known_vectors() {
        assert_eq!(h(b""), "da39a3ee5e6b4b0d3255bfef95601890afd80709");
        assert_eq!(h(b"abc"), "a9993e364706816aba3e25717850c26c9cd0d89d");
        // split updates equal one update
        let data: Vec<u8> = (0..1000u32).map(|i| i as u8).collect();
        let mut s = Sha1::default();
        for c in data.chunks(37) {
            s.update(c);
        }
        assert_eq!(hex(&s.finish()), h(&data));
    }
}
