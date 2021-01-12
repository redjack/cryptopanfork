//! Pure Rust implementation of "B64" encoding, a minified subset of the standard
//! Base64 encoding (RFC 4648, section 4) used by the PHC string format.
//!
//! Omits padding (`=`) as well as extra whitespace, as described in the PHC string
//! format specification:
//!
//! <https://github.com/P-H-C/phc-string-format/blob/master/phc-sf-spec.md#b64>
//!
//! Supports the Base64 character subset: `[A-Z]`, `[a-z]`, `[0-9]`, `+`, `/`
//!
//! Implemented without data-dependent branches or look up tables, thereby
//! providing "best effort" constant-time operation. Adapted from the following
//! constant-time C++ implementation of Base64:
//!
//! <https://github.com/Sc00bz/ConstTimeEncoding/blob/master/base64.cpp>
//!
//! Copyright (c) 2014 Steve "Sc00bz" Thomas (steve at tobtu dot com).
//! Derived code is dual licensed MIT + Apache 2 (with permission from Sc00bz).

#![no_std]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/RustCrypto/meta/master/logo_small.png",
    html_favicon_url = "https://raw.githubusercontent.com/RustCrypto/meta/master/logo.svg"
)]
#![warn(missing_docs, rust_2018_idioms)]

#[cfg(feature = "alloc")]
#[macro_use]
extern crate alloc;

#[cfg(feature = "std")]
extern crate std;

use core::str;

#[cfg(feature = "alloc")]
use alloc::{string::String, vec::Vec};

mod errors;

pub use errors::{Error, InvalidEncodingError, InvalidLengthError};

/// Encode the input byte slice as "B64", writing the result into the provided
/// destination slice, and returning an ASCII-encoded string value.
pub fn encode<'a>(src: &[u8], dst: &'a mut [u8]) -> Result<&'a str, InvalidLengthError> {
    let elen = encoded_len(src);
    if elen > dst.len() {
        return Err(InvalidLengthError);
    }
    let dst = &mut dst[..elen];

    let mut src_chunks = src.chunks_exact(3);
    let mut dst_chunks = dst.chunks_exact_mut(4);
    for (s, d) in (&mut src_chunks).zip(&mut dst_chunks) {
        encode_3bytes(s, d);
    }
    let src_rem = src_chunks.remainder();
    let dst_rem = dst_chunks.into_remainder();

    let mut tmp_in = [0u8; 3];
    let mut tmp_out = [0u8; 4];
    tmp_in[..src_rem.len()].copy_from_slice(src_rem);
    encode_3bytes(&tmp_in, &mut tmp_out);
    dst_rem.copy_from_slice(&tmp_out[..dst_rem.len()]);

    debug_assert!(str::from_utf8(dst).is_ok());
    // SAFETY: values written by `encode_3bytes` are valid one-byte UTF-8 chars
    Ok(unsafe { str::from_utf8_unchecked(dst) })
}

/// Encode the input byte slice as a "B64"-encoded [`String`].
#[cfg(feature = "alloc")]
#[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
pub fn encode_string(input: &[u8]) -> String {
    let elen = encoded_len(input);
    let mut dst = vec![0u8; elen];
    let res = encode(input, &mut dst);
    debug_assert_eq!(elen, res.unwrap().len());
    debug_assert!(str::from_utf8(&dst).is_ok());
    // SAFETY: `dst` is fully written and contains only valid one-byte UTF-8 chars
    unsafe { String::from_utf8_unchecked(dst) }
}

/// Get the "B64"-encoded length of the given byte slice.
pub const fn encoded_len(bytes: &[u8]) -> usize {
    let q = bytes.len() * 4;
    let r = q % 3;
    (q / 3) + (r != 0) as usize
}

/// "B64" decode the given source byte slice into the provided destination
/// buffer.
pub fn decode<'a>(src: &str, dst: &'a mut [u8]) -> Result<&'a [u8], Error> {
    let dlen = decoded_len(src);
    if dlen > dst.len() {
        return Err(Error::InvalidLength);
    }
    let src = src.as_bytes();
    let dst = &mut dst[..dlen];

    let mut err: isize = 0;

    let mut src_chunks = src.chunks_exact(4);
    let mut dst_chunks = dst.chunks_exact_mut(3);
    for (s, d) in (&mut src_chunks).zip(&mut dst_chunks) {
        err |= decode_3bytes(s, d);
    }
    let src_rem = src_chunks.remainder();
    let dst_rem = dst_chunks.into_remainder();

    err |= !(src_rem.is_empty() || src_rem.len() >= 2) as isize;
    let mut tmp_out = [0u8; 3];
    let mut tmp_in = [b'A'; 4];
    tmp_in[..src_rem.len()].copy_from_slice(src_rem);
    err |= decode_3bytes(&tmp_in, &mut tmp_out);
    dst_rem.copy_from_slice(&tmp_out[..dst_rem.len()]);

    if err == 0 {
        Ok(dst)
    } else {
        Err(Error::InvalidEncoding)
    }
}

/// Decode a "B64"-encoded string into a byte vector.
#[cfg(feature = "alloc")]
#[cfg_attr(docsrs, doc(cfg(feature = "alloc")))]
pub fn decode_vec(input: &str) -> Result<Vec<u8>, InvalidEncodingError> {
    let dlen = decoded_len(input);
    let mut output = vec![0u8; dlen];
    match decode(input, &mut output) {
        Ok(v) => debug_assert_eq!(dlen, v.len()),
        Err(Error::InvalidEncoding) => return Err(InvalidEncodingError),
        Err(Error::InvalidLength) => unreachable!(),
    }
    Ok(output)
}

/// Get the length of the output from decoding the provided "B64"-encoded input.
pub const fn decoded_len(bytes: &str) -> usize {
    (bytes.len() * 3) / 4
}

// B64 character set:
// [A-Z]      [a-z]      [0-9]      +     /
// 0x41-0x5a, 0x61-0x7a, 0x30-0x39, 0x2b, 0x2f

#[inline(always)]
fn encode_3bytes(src: &[u8], dst: &mut [u8]) {
    debug_assert_eq!(src.len(), 3);
    debug_assert!(dst.len() >= 4, "dst too short: {}", dst.len());

    let b0 = src[0] as isize;
    let b1 = src[1] as isize;
    let b2 = src[2] as isize;

    dst[0] = encode_6bits(b0 >> 2);
    dst[1] = encode_6bits(((b0 << 4) | (b1 >> 4)) & 63);
    dst[2] = encode_6bits(((b1 << 2) | (b2 >> 6)) & 63);
    dst[3] = encode_6bits(b2 & 63);
}

#[inline(always)]
fn encode_6bits(src: isize) -> u8 {
    let mut diff = 0x41isize;

    // if (in > 25) diff += 0x61 - 0x41 - 26; // 6
    diff += ((25isize - src) >> 8) & 6;

    // if (in > 51) diff += 0x30 - 0x61 - 26; // -75
    diff -= ((51isize - src) >> 8) & 75;

    // if (in > 61) diff += 0x2b - 0x30 - 10; // -15
    diff -= ((61isize - src) >> 8) & 15;

    // if (in > 62) diff += 0x2f - 0x2b - 1; // 3
    diff += ((62isize - src) >> 8) & 3;

    (src + diff) as u8
}

#[inline(always)]
fn decode_3bytes(src: &[u8], dst: &mut [u8]) -> isize {
    debug_assert_eq!(src.len(), 4);
    debug_assert!(dst.len() >= 3, "dst too short: {}", dst.len());

    let c0 = decode_6bits(src[0]);
    let c1 = decode_6bits(src[1]);
    let c2 = decode_6bits(src[2]);
    let c3 = decode_6bits(src[3]);

    dst[0] = ((c0 << 2) | (c1 >> 4)) as u8;
    dst[1] = ((c1 << 4) | (c2 >> 2)) as u8;
    dst[2] = ((c2 << 6) | c3) as u8;

    ((c0 | c1 | c2 | c3) >> 8) & 1
}

#[inline(always)]
fn decode_6bits(src: u8) -> isize {
    let ch = src as isize;
    let mut ret: isize = -1;

    // if (ch > 0x40 && ch < 0x5b) ret += ch - 0x41 + 1; // -64
    ret += (((64isize - ch) & (ch - 91isize)) >> 8) & (ch - 64isize);

    // if (ch > 0x60 && ch < 0x7b) ret += ch - 0x61 + 26 + 1; // -70
    ret += (((96isize - ch) & (ch - 123isize)) >> 8) & (ch - 70isize);

    // if (ch > 0x2f && ch < 0x3a) ret += ch - 0x30 + 52 + 1; // 5
    ret += (((47isize - ch) & (ch - 58isize)) >> 8) & (ch + 5isize);

    // if (ch == 0x2b) ret += 62 + 1;
    ret += (((42isize - ch) & (ch - 44isize)) >> 8) & 63;

    // if (ch == 0x2f) ret += 63 + 1;
    ret + ((((46isize - ch) & (ch - 48isize)) >> 8) & 64)
}