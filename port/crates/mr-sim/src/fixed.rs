//! Fixed-point helpers that reproduce the R3000 code the compiler emitted.
//!
//! The game's C divides by powers of two with the "round toward zero" idiom
//! (`if (x < 0) x += 2^n - 1; x >>= n`), divides with MIPS `div` (truncating,
//! traps on zero), multiplies with `mult` keeping the low 32 bits, and stores
//! halfwords by truncation. Every helper here names the idiom it stands for.

/// `x / 2^n` as the compiler emits it for signed C division (bias then sra).
#[inline]
pub fn div_pow2(x: i32, n: u32) -> i32 {
    if x < 0 {
        x.wrapping_add((1 << n) - 1) >> n
    } else {
        x >> n
    }
}

/// MIPS `div` quotient (truncates toward zero). The game traps (`break
/// 0x1c00`) on a zero divisor; reaching that is a bug in the port.
#[inline]
pub fn div(a: i32, b: i32) -> i32 {
    assert!(b != 0, "division by zero (the game would trap: break 0x1c00)");
    a.wrapping_div(b)
}

/// Low 32 bits of a `mult` product.
#[inline]
pub fn mul(a: i32, b: i32) -> i32 {
    a.wrapping_mul(b)
}

/// Wrap a 12-bit angle difference into -0x800..=0x800 the way the game does
/// (`if d > 0x800 d -= 0x1000; else if d < -0x800 d += 0x1000`).
#[inline]
pub fn wrap_angle(d: i32) -> i32 {
    if d > 0x800 {
        d - 0x1000
    } else if d < -0x800 {
        d + 0x1000
    } else {
        d
    }
}

/// Halfword store then signed reload: `(short)x`.
#[inline]
pub fn s16(x: i32) -> i32 {
    x as i16 as i32
}

/// Byte store then signed reload: `(char)x`.
#[inline]
pub fn s8(x: i32) -> i32 {
    x as i8 as i32
}

/// Byte store then unsigned reload: `(byte)x`.
#[inline]
pub fn u8_(x: i32) -> i32 {
    x as u8 as i32
}

/// The per-tick sub-step loop the game writes as
/// `n = dt & 0xffff; do { ...; n--; } while (0 < (int)(n << 16));`.
/// Returns how many times the body runs (dt = 2 -> 2; dt = 0 -> 1, since the
/// body runs once before the test).
#[inline]
pub fn substeps(dt: i32) -> u32 {
    let mut n = dt as u32 & 0xFFFF;
    let mut k = 0;
    loop {
        k += 1;
        n = n.wrapping_sub(1);
        if (n << 16) as i32 <= 0 {
            break;
        }
    }
    k
}
