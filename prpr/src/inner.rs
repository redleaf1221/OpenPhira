//! Byte-level transforms shared by the proprietary parts of `prpr`.
//!
//! Upstream ships this file only in `--cfg closed` builds.  It is not
//! cryptography: a 16-point Walsh–Hadamard butterfly over `u8` followed by a
//! bit-reversal permutation, with no key material at all.  The real gate on
//! score submissions is the HMAC in `scene::game::inner`, plus the fact that the
//! server can recompute every derived value from the counts.

/// Block size of the record cipher.
pub const BLOCK: usize = 16;

/// Encrypt `data` for the score-upload token.
///
/// The plaintext is padded to a multiple of [`BLOCK`] with `8 + pad`, so the
/// payload length is recoverable from the last byte of the last block
/// (`len = padded_len - (last - 8)`).
pub fn custom_enc(data: &[u8]) -> Vec<u8> {
    let mut pad = (-(data.len() as i64)).rem_euclid(BLOCK as i64) as usize;
    if pad == 0 {
        // A full block is appended even when the input is already aligned, so
        // the last byte always carries the filler value.
        pad = BLOCK;
    }

    let mut b = Vec::with_capacity(data.len() + pad);
    b.extend_from_slice(data);
    b.resize(data.len() + pad, (8 + pad) as u8);

    for block in b.chunks_exact_mut(BLOCK) {
        forward_transform(block);
    }
    b
}

/// Decrypt a record encrypted by [`custom_enc`].
///
/// The client never needs this direction, but the server does, and having it
/// here keeps the transform verifiable in tests.
pub fn custom_dec(data: &[u8]) -> Vec<u8> {
    let mut b = data.to_vec();
    let aligned = b.len() - b.len() % BLOCK;
    for block in b[..aligned].chunks_exact_mut(BLOCK) {
        inverse_transform(block);
    }

    if let Some(&marker) = b.last() {
        let plain_len = b.len() as i64 - (marker as i64 - 8);
        if 0 <= plain_len && (plain_len as usize) <= b.len() {
            b.truncate(plain_len as usize);
        }
    }
    b
}

/// One forward block: butterfly stages `1, 2, 4, 8`, then `i <-> rbit4(i)`.
fn forward_transform(b: &mut [u8]) {
    for k in [1usize, 2, 4, 8] {
        for i in 0..BLOCK {
            if i & k != 0 {
                b[i] = b[i].wrapping_add(b[i & !k]);
            }
        }
    }
    for i in 0..BLOCK {
        let j = i.reverse_bits() >> (usize::BITS - 4);
        if i < j {
            b.swap(i, j);
        }
    }
}

/// One inverse block: undo the permutation (it is an involution), then the
/// butterfly stages with subtraction in reverse order.
fn inverse_transform(b: &mut [u8]) {
    for i in 0..BLOCK {
        let j = i.reverse_bits() >> (usize::BITS - 4);
        if i < j {
            b.swap(i, j);
        }
    }
    for k in [8usize, 4, 2, 1] {
        for i in (0..BLOCK).rev() {
            if i & k != 0 {
                b[i] = b[i].wrapping_sub(b[i & !k]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        for len in (0..80).chain([92, 96, 128, 1000]) {
            let plain: Vec<u8> = (0..len).map(|i| (i * 11 + len) as u8).collect();
            assert_eq!(custom_dec(&custom_enc(&plain)), plain, "len = {len}");
        }
    }

    #[test]
    fn padding_is_always_appended() {
        assert_eq!(custom_enc(&[0; 16]).len(), 32);
        assert_eq!(custom_enc(&[]).len(), 16);
        assert_eq!(custom_enc(&[0; 92]).len(), 96);
    }

    #[test]
    fn filler_byte_is_eight_plus_pad() {
        // 92 = 5 * 16 + 12, so pad = 4 and every filler byte is 8 + 4 = 12.
        let mut manual = vec![0u8; 92];
        manual.extend_from_slice(&[12u8; 4]);
        for block in manual.chunks_exact_mut(BLOCK) {
            forward_transform(block);
        }
        assert_eq!(custom_enc(&[0; 92]), manual);
    }

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    /// A 92-byte record and the 96 bytes `custom_enc` makes of it.
    ///
    /// The vector is synthetic (player 424242, chart 4242) so no real account
    /// appears in this repository; a real captured pair was used to validate the
    /// implementation, and this pins the result so a regression cannot pass.
    /// A round-trip test alone would not: it passes even when both directions
    /// are wrong.
    #[test]
    fn matches_the_reference_pair() {
        let record = unhex(
            "033279060000001092e6b5faf8b048893f00000000a43c6e00000000d82c010000\
             a80100002a00000000000000040000006428a9dadfa60de7ea52a0122b81b634\
             394f35790f87f981c2ce8779751e2ad74074d2d601000000000000",
        );
        let cipher = unhex(
            "0395038d7cc37c0335ad3555b4dbc4643f3f3f173f3f7b543f3fe3e73f3f8d92\
             0000000001010101a8a8d2d6a9a9d3d700e7dad32861a87464351d2b35f8a1d8\
             34b5ada783d283696db0f526f189f9c7d7d7adb94b4b21391717ee065d5d3464",
        );
        assert_eq!(record.len(), 92);
        assert_eq!(cipher.len(), 96);
        assert_eq!(custom_enc(&record), cipher);
        assert_eq!(custom_dec(&cipher), record);
    }
}
