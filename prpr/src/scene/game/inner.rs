//! Score-upload token.
//!
//! Recovered from `libphira.so:game__inner__encode_record` (0xD24860).  The
//! shipping client builds a fixed 92-byte record, encrypts it with the 16-byte
//! block cipher from [`crate::inner`], appends an HMAC-SHA256 tag over the
//! ciphertext and deflates the result:
//!
//! ```text
//! token = raw_deflate( custom_enc(record) || HMAC-SHA256(KEY, custom_enc(record)) )
//! ```
//!
//! Everything the server needs to recompute the score (`counts`, `max_combo`,
//! `num_of_notes`) is therefore in-band, and `accuracy`/`score` are derived
//! rather than transmitted.

use hmac::{Hmac, Mac};
use sha2::Sha256;

use super::GameScene;
use crate::inner::custom_enc;
use crate::judge::inner::device;

/// Record format version.  The first payload byte; bump only if the layout
/// changes, since the server dispatches on it.
const VERSION: u8 = 3;

/// Length of the plaintext record.
const PLAIN_LEN: usize = 92;

/// HMAC key baked into the client.  Stored XOR-masked at `0x320074` in
/// `libphira.so` and demasked with four 64-bit words plus two tail bytes.
const KEY: &[u8] = b"KTCfLeu6d6gq957WOEyj-light";

/// Compression level handed to the deflate encoder.  The client uses zlib's
/// default; for a 128-byte high-entropy payload every level picks the same
/// stored block, so this only matters if the payload ever becomes compressible.
const DEFLATE_LEVEL: u8 = 6;

type HmacSha256 = Hmac<Sha256>;

/// Build the upload token for a finished play.
///
/// `player_id` and `chart_id` are passed in rather than read from `game` so the
/// caller decides which identity/chart is being rated.  The id is an `i32`
/// because that is what `BasicPlayer` stores; the wire format is just its
/// little-endian bit pattern.
pub fn encode_record(game: &GameScene, player_id: i32, chart_id: i32) -> Vec<u8> {
    let judge = &game.judge.inner;
    let config = &game.res.config;

    let mut payload = Vec::with_capacity(PLAIN_LEN);

    payload.push(VERSION);
    payload.extend_from_slice(&player_id.to_le_bytes());
    // Big-endian on purpose: the server reads the chart id as a network-order
    // word, unlike every other integer in this record.
    payload.extend_from_slice(&chart_id.to_be_bytes());
    // Variance of the hit offsets; the server takes the square root to get the
    // deviation it displays.  `Miss` counts as a 0.25 offset.
    payload.extend_from_slice(&judge.diff_variance().to_le_bytes());
    // Mod bits, sent twice (here and in the trailer) so a tampered middle can be
    // detected by cross-checking.
    payload.extend_from_slice(&(config.mods.bits() as u32).to_le_bytes());

    // Three flags hidden in the low bit of a random byte; the high seven bits
    // are fresh noise on every encoding.  All three are *anomaly* flags, i.e.
    // zero for a play that really happened at 1.0x:
    //   +21  the judge never advanced  (`trunc(|last_time| * 2^14) == 0`)
    //   +22  the play ran below 1.0x  (`trunc(speed * 2^14) < 2^14`)
    //   +23  the counters disagree    (`sum(counts) != num_of_notes`)
    // The last one is *not* "no notes": the binary compares the protected note
    // count against the sum of the four counters, which is how a record whose
    // counts were edited gets flagged.
    payload.push(noise_flag(last_time_is_unset(game)));
    payload.push(noise_flag(config.speed < 1.0));
    payload.push(noise_flag(judged_notes(judge) != judge.num_of_notes()));

    // Device attestation word plus the same flags again in the trailer.
    payload.extend_from_slice(&device::fingerprint().to_le_bytes());
    payload.push(0xD8 | u8::from(device::virtual_input()));
    payload.extend_from_slice(&judge.max_combo().to_le_bytes());
    for count in judge.counts() {
        payload.extend_from_slice(&count.to_le_bytes());
    }
    payload.push(speed_percent(config.speed));

    // SHA-256 midstate of the chart file: commits the client to the exact bytes
    // it played, so a forged record cannot reuse another chart's hash.
    payload.extend_from_slice(&chart_midstate(&game.chart_bytes));

    payload.push(b't' | u8::from(device::virtual_input()));
    payload.push(0xD2 | u8::from(device::tampered()));
    // Sum of the four counters, not the note count: confirmed on device, where
    // the counters add up to the note count and that sum lands at this offset.
    payload.extend_from_slice(&judged_notes(judge).to_le_bytes());
    payload.extend_from_slice(&(config.mods.bits() as u32).to_le_bytes());

    debug_assert_eq!(payload.len(), PLAIN_LEN);

    let ciphertext = custom_enc(&payload);
    let mut mac = HmacSha256::new_from_slice(KEY).expect("HMAC accepts keys of any length");
    mac.update(&ciphertext);
    let tag = mac.finalize().into_bytes();

    // Wire format: the token is just `deflate(ciphertext || tag)`, so its length
    // depends on how well that 128-byte payload compresses.
    //
    // The captured play produced 133 bytes because the payload did not compress:
    // deflate fell back to a *stored block*, whose five-byte header is the
    // `01 80 00 7f ff` that looks like a magic constant:
    //   0x01            BFINAL=1, BTYPE=00 (stored)
    //   0x0080          LEN  = 128 (little endian)
    //   0xff7f          NLEN = ~128 (little endian)
    // followed by the 128 payload bytes.  A record whose ids and counters are
    // mostly zero does compress, and then the token is shorter than 128 and that
    // header does not exist at all -- so the reader must inflate, never strip a
    // fixed prefix.
    let mut sealed = Vec::with_capacity(ciphertext.len() + tag.len());
    sealed.extend_from_slice(&ciphertext);
    sealed.extend_from_slice(&tag);
    seal(&sealed)
}

/// Raw-deflate (no zlib wrapper, no checksum), matching `flate2`'s
/// `DeflateEncoder` as used by the client.
fn seal(bytes: &[u8]) -> Vec<u8> {
    miniz_oxide::deflate::compress_to_vec(bytes, DEFLATE_LEVEL)
}

/// `round(speed * 100)`, clamped to 200 (2.0×), as a single byte.
fn speed_percent(speed: f32) -> u8 {
    let v = (speed * 20.0).round().max(0.0) as u32 * 5;
    v.min(200) as u8
}

/// The client stores "the judge never advanced" rather than "the judge advanced",
/// so this is `1` for a record that was not actually played.
fn last_time_is_unset(game: &GameScene) -> bool {
    // The binary computes `trunc(|last_time| * 2^14) == 0`; keep the same
    // fixed-point comparison so the boundary matches bit for bit.
    (game.judge.last_time.abs() * 16384.0) as u64 == 0
}

/// `sum(counts)`: what the record carries at `+84`, separate from
/// `JudgeInner::num_of_notes`.
fn judged_notes(judge: &crate::judge::inner::JudgeInner) -> u32 {
    judge.counts().iter().sum()
}

fn noise_flag(flag: bool) -> u8 {
    (rand::random::<u32>() as u8 & 0xFE) | u8::from(flag)
}

// ---------------------------------------------------------------------------
// SHA-256 midstate
// ---------------------------------------------------------------------------

const SHA256_IV: [u32; 8] = [
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
];

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

/// SHA-256 state after all *complete* 64-byte blocks of `bytes`.
///
/// This is deliberately not the digest: the client reads the eight state words
/// out before the padding block is added, and stores them little-endian.  Any
/// trailing partial block is still sitting in the hash object's buffer and is
/// ignored on both sides, so the value is a pure function of the chart file.
fn chart_midstate(bytes: &[u8]) -> [u8; 32] {
    let mut state = SHA256_IV;
    let mut block = [0u8; 64];

    let mut chunks = bytes.chunks_exact(64);
    for chunk in &mut chunks {
        block.copy_from_slice(chunk);
        compress(&mut state, &block);
    }

    let mut out = [0u8; 32];
    for (i, word) in state.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.to_le_bytes());
    }
    out
}

fn compress(state: &mut [u32; 8], block: &[u8; 64]) {
    let mut w = [0u32; 64];
    for i in 0..16 {
        w[i] = u32::from_be_bytes([block[i * 4], block[i * 4 + 1], block[i * 4 + 2], block[i * 4 + 3]]);
    }
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }

    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for i in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ (!e & g);
        let t1 = h
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(SHA256_K[i])
            .wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(maj);

        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }

    for (slot, value) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *slot = slot.wrapping_add(value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::Digest;

    /// A 92-byte record, its ciphertext, its tag and the sealed token.
    ///
    /// Synthetic on purpose (player 424242, chart 4242): the real captured play
    /// was used to validate everything here, but its record carries an account
    /// id and a chart id, so it is not shipped.  The shape is identical —
    /// 92 -> 96 -> 133 with a stored block — so a regression still fails.
    const RECORD: &str = "033279060000001092e6b5faf8b048893f00000000a43c6e00000000d82c010000a80100002a00000000000000040000006428a9dadfa60de7ea52a0122b81b634394f35790f87f981c2ce8779751e2ad74074d2d601000000000000";
    const CIPHER: &str = "0395038d7cc37c0335ad3555b4dbc4643f3f3f173f3f7b543f3fe3e73f3f8d920000000001010101a8a8d2d6a9a9d3d700e7dad32861a87464351d2b35f8a1d834b5ada783d283696db0f526f189f9c7d7d7adb94b4b21391717ee065d5d3464";
    const TAG: &str = "8d6e10e0e8ce0105720fee42cf83db27b1f948de8d9e2bd33f0eeb11671d4f68";
    const TOKEN: &str = "0180007fff0395038d7cc37c0335ad3555b4dbc4643f3f3f173f3f7b543f3fe3e73f3f8d920000000001010101a8a8d2d6a9a9d3d700e7dad32861a87464351d2b35f8a1d834b5ada783d283696db0f526f189f9c7d7d7adb94b4b21391717ee065d5d34648d6e10e0e8ce0105720fee42cf83db27b1f948de8d9e2bd33f0eeb11671d4f68";

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
    }

    /// The record, and what comes out of the cipher.
    #[test]
    fn the_record_encrypts_to_the_reference_ciphertext() {
        assert_eq!(hex(RECORD).len(), PLAIN_LEN);
        assert_eq!(custom_enc(&hex(RECORD)), hex(CIPHER));
    }

    /// An oracle for `seal`: the 133-byte token really is
    /// raw-deflate of `ciphertext || tag`, including the "magic" first bytes.
    ///
    /// This is the test that settles the 96-vs-128-vs-133 question.  If the
    /// encoder ever stopped emitting a stored block the first five bytes would
    /// change, so the assertion pins both the framing and the payload.
    #[test]
    fn the_token_is_just_the_ciphertext_and_tag_after_deflate() {
        let sealed = seal(&hex(&format!("{CIPHER}{TAG}")));
        assert_eq!(sealed, hex(TOKEN));
        assert_eq!(&sealed[..5], &[0x01, 0x80, 0x00, 0x7f, 0xff]);
        assert_eq!(sealed.len(), 133);
    }

    /// The last 32 bytes of the ciphertext/tag blob really are `HMAC(KEY, ct)`.
    #[test]
    fn the_tag_covers_the_ciphertext() {
        let ct = hex(CIPHER);
        let mut mac = HmacSha256::new_from_slice(KEY).unwrap();
        mac.update(&ct);
        let tag = mac.finalize().into_bytes();
        assert_eq!(&hex(TOKEN)[101..], &tag[..]);
    }

    /// `chart_midstate` must agree with a reference SHA-256 that was *not*
    /// written by us: take the midstate, add the padding block by hand, and the
    /// result has to be the digest the `sha2` crate produces.  A broken
    /// `compress` cannot survive this, unlike a round-trip against itself.
    #[test]
    fn midstate_is_the_state_sha256_holds_before_padding() {
        let data: Vec<u8> = (0u8..64).collect();
        let mid = chart_midstate(&data);

        let mut state = [0u32; 8];
        for (i, word) in state.iter_mut().enumerate() {
            *word = u32::from_le_bytes(mid[i * 4..i * 4 + 4].try_into().unwrap());
        }
        let mut pad = [0u8; 64];
        pad[0] = 0x80;
        pad[56..].copy_from_slice(&(64u64 * 8).to_be_bytes());
        compress(&mut state, &pad);

        let mut digest = [0u8; 32];
        for (i, word) in state.iter().enumerate() {
            digest[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
        }
        assert_eq!(&digest[..], &Sha256::digest(&data)[..]);
    }

    /// A trailing partial block is left in the hash object's buffer and never
    /// compressed, so it must not influence the midstate.
    #[test]
    fn a_partial_tail_is_ignored() {
        let mut a = vec![0x41u8; 128];
        assert_eq!(chart_midstate(&a), chart_midstate(&a[..128]));
        a.extend_from_slice(&[0xFF; 7]);
        assert_eq!(chart_midstate(&a[..128]), chart_midstate(&a));
    }

    #[test]
    fn speed_percent_matches_the_client() {
        assert_eq!(speed_percent(1.0), 100);
        assert_eq!(speed_percent(1.2), 120);
        assert_eq!(speed_percent(0.9), 90);
        assert_eq!(speed_percent(2.5), 200);
    }
}
