//! mr-audio: the game's sound from the owner's disc, std only.
//!
//! - `adpcm`: the PS1 SPU ADPCM block (16-byte blocks, 28 samples), shared by
//!   VAG/VB sample banks.
//! - `xa`: CD-XA ADPCM (TUNES/*.XA race music, VOICES/*.XA), interleaved
//!   channels demuxed from the Mode 2 Form 2 sectors.
//! - `vab`: SFX/SFX_6.VH + .VB sample bank (programs, tones, samples).
//! - `spu`: a 24-voice SPU model (pitch, ADSR, volume) that plays key-on
//!   events from the simulation.
//!
//! Specs: docs/spec/audio.md.

pub mod adpcm;
pub mod spu;
pub mod vab;
pub mod xa;
