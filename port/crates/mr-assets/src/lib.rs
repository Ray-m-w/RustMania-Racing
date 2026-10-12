//! mr-assets: decoders for Muppet RaceMania's own file formats, read from the
//! owner's disc at run time (mr-disc). std-only (ADR-0002). One module per
//! format; each module's doc comment cites the game function(s) whose
//! behavior it reproduces and how it was verified against the recomp.
//! Format specs: docs/spec/formats/.

pub mod img;
pub mod raw;
pub mod dat;
pub mod ter;
pub mod axe;
pub mod mdl;
pub mod anm;
pub mod vram;

/// Little-endian readers shared by the decoders.
pub mod le {
    pub fn u16(b: &[u8], o: usize) -> u16 {
        u16::from_le_bytes([b[o], b[o + 1]])
    }
    pub fn i16(b: &[u8], o: usize) -> i16 {
        u16(b, o) as i16
    }
    pub fn u32(b: &[u8], o: usize) -> u32 {
        u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
    }
    pub fn i32(b: &[u8], o: usize) -> i32 {
        u32(b, o) as i32
    }
}
