//! A tiny PS1 memory model (2 MB main RAM + 1 KB scratchpad) used to move
//! state between the simulation's named structs and the game's own layout:
//! the oracle harness builds one from a capture, the simulation never needs
//! it. No I/O here.

#[derive(Clone)]
pub struct Mem {
    pub ram: Vec<u8>,
    pub scratch: Vec<u8>,
}

impl Mem {
    pub fn new(ram: Vec<u8>, scratch: Vec<u8>) -> Mem {
        assert!(ram.len() >= 0x20_0000 && scratch.len() >= 0x400);
        Mem { ram, scratch }
    }

    fn loc(&self, a: u32) -> (bool, usize) {
        let p = a & 0x1FFF_FFFF;
        if (0x1F80_0000..0x1F80_0400).contains(&p) {
            (true, (p - 0x1F80_0000) as usize)
        } else {
            assert!(p < 0x80_0000, "address {a:08X} is not RAM");
            (false, (p & 0x1F_FFFF) as usize)
        }
    }

    pub fn slice(&self, a: u32, n: usize) -> &[u8] {
        let (s, o) = self.loc(a);
        if s {
            &self.scratch[o..o + n]
        } else {
            &self.ram[o..o + n]
        }
    }

    pub fn slice_mut(&mut self, a: u32, n: usize) -> &mut [u8] {
        let (s, o) = self.loc(a);
        if s {
            &mut self.scratch[o..o + n]
        } else {
            &mut self.ram[o..o + n]
        }
    }

    pub fn u8(&self, a: u32) -> u8 {
        self.slice(a, 1)[0]
    }
    pub fn i8(&self, a: u32) -> i8 {
        self.u8(a) as i8
    }
    pub fn u16(&self, a: u32) -> u16 {
        u16::from_le_bytes(self.slice(a, 2).try_into().unwrap())
    }
    pub fn i16(&self, a: u32) -> i16 {
        self.u16(a) as i16
    }
    pub fn u32(&self, a: u32) -> u32 {
        u32::from_le_bytes(self.slice(a, 4).try_into().unwrap())
    }
    pub fn i32(&self, a: u32) -> i32 {
        self.u32(a) as i32
    }
    pub fn put_u8(&mut self, a: u32, v: u8) {
        self.slice_mut(a, 1)[0] = v;
    }
    pub fn put_u16(&mut self, a: u32, v: u16) {
        self.slice_mut(a, 2).copy_from_slice(&v.to_le_bytes());
    }
    pub fn put_u32(&mut self, a: u32, v: u32) {
        self.slice_mut(a, 4).copy_from_slice(&v.to_le_bytes());
    }
    pub fn write(&mut self, a: u32, d: &[u8]) {
        self.slice_mut(a, d.len()).copy_from_slice(d);
    }
}
