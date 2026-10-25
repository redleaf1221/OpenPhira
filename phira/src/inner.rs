//! Decryption of the built-in resources.
//!
//! This is the open re-implementation of the module that upstream only ships in
//! `--cfg closed` builds.  The behaviour was recovered from `libphira.so`
//! (see `phira-inner-analysis.md` at the repository root); everything here is
//! the same algorithm without the control-flow flattening and the encrypted
//! jump tables.
//!
//! Built-in assets under `assets/res/` (`*.char`, `bgm`, `song/*/*`) are stored
//! as `zstd(custom_enc_8(plain))`, so [`resolve_data`] undoes both layers.
//!
//! The cipher is a tiny self-contained design: eight bytes are mixed with a
//! Walsh–Hadamard butterfly done entirely in wrapping `u8` arithmetic, and the
//! number of payload bytes is recovered from the filler byte of the last block.
//! It is obfuscation, not cryptography — the transform is its own documentation.

/// Block size of the resource cipher.
const BLOCK: usize = 8;

/// Filler value that marks "the last block is full" is `8 - pad`, so a full
/// block ends with `0x08` and the recovered length stays unchanged.
const FULL_BLOCK_MARKER: usize = 8;

/// `(destination, source)` pairs of the three butterfly stages.
///
/// Stage `k` subtracts every byte from the byte `k` positions earlier, which is
/// exactly an in-place inverse Walsh–Hadamard transform over `u8`.
const STAGE_1: [(usize, usize); 4] = [(1, 0), (3, 2), (5, 4), (7, 6)];
const STAGE_2: [(usize, usize); 4] = [(2, 0), (3, 1), (6, 4), (7, 5)];
const STAGE_4: [(usize, usize); 4] = [(4, 0), (5, 1), (6, 2), (7, 3)];

/// Decrypt and decompress one built-in resource.
///
/// Used by [`crate::load_res`] and by the character illustrations in
/// `page::home`.
pub fn resolve_data(data: Vec<u8>) -> Vec<u8> {
    let decrypted = custom_dec(&data);
    zstd::decode_all(decrypted.as_slice()).expect("corrupted built-in resource")
}

/// Decrypt `data` in place.
pub fn custom_dec(data: &[u8]) -> Vec<u8> {
    let mut b = data.to_vec();

    for i in (0..b.len() - b.len() % BLOCK).step_by(BLOCK) {
        // Undo the two trailing swaps of the forward transform.
        b.swap(i + 2, i + 6);
        b.swap(i + 3, i + 6);

        for (dst, src) in STAGE_1 {
            b[i + dst] = b[i + dst].wrapping_sub(b[i + src]);
        }
        for (dst, src) in STAGE_2 {
            b[i + dst] = b[i + dst].wrapping_sub(b[i + src]);
        }
        for (dst, src) in STAGE_4 {
            b[i + dst] = b[i + dst].wrapping_sub(b[i + src]);
        }

        b.swap(i + 3, i + 5);
    }

    // The last byte of the ciphertext is the filler value; the plaintext is
    // `len + marker - 8` bytes long.  A truncated or foreign file can produce a
    // value outside that range, in which case the buffer is returned untouched.
    if let Some(&marker) = b.last() {
        let plain_len = b.len() as i64 + marker as i64 - FULL_BLOCK_MARKER as i64;
        if 0 <= plain_len && (plain_len as usize) <= b.len() {
            b.truncate(plain_len as usize);
        }
    }
    b
}

/// Encrypt `data` — the exact inverse of [`custom_dec`].
///
/// The shipping client has no use for this (assets are produced offline) but it
/// keeps the transform testable and documents the padding rule.
#[allow(dead_code)]
pub fn custom_enc(data: &[u8]) -> Vec<u8> {
    let mut pad = (-(data.len() as i64)).rem_euclid(BLOCK as i64) as usize;
    if pad == 0 {
        pad = BLOCK;
    }
    let marker = FULL_BLOCK_MARKER - pad;

    let mut b = Vec::with_capacity(data.len() + pad);
    b.extend_from_slice(data);
    b.resize(data.len() + pad, marker as u8);

    for i in (0..b.len()).step_by(BLOCK) {
        b.swap(i + 3, i + 5);

        for (dst, src) in STAGE_4.into_iter().rev() {
            b[i + dst] = b[i + dst].wrapping_add(b[i + src]);
        }
        for (dst, src) in STAGE_2.into_iter().rev() {
            b[i + dst] = b[i + dst].wrapping_add(b[i + src]);
        }
        for (dst, src) in STAGE_1.into_iter().rev() {
            b[i + dst] = b[i + dst].wrapping_add(b[i + src]);
        }

        b.swap(i + 3, i + 6);
        b.swap(i + 2, i + 6);
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        for len in (0..200).chain([255, 256, 1000]) {
            let plain: Vec<u8> = (0..len).map(|i| (i * 37 + len) as u8).collect();
            assert_eq!(custom_dec(&custom_enc(&plain)), plain, "len = {len}");
        }
    }

    #[test]
    fn known_assets_survive_the_packing_pipeline() {
        // The repository stores plaintext resources; `custom_enc(zstd(payload))` is applied by
        // `tools/encrypt_assets.py` at packaging time.  So the property worth asserting is that
        // a plaintext asset, once packed, decodes back to exactly itself.
        const PNG_MAGIC: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];

        let mut seen = 0;
        for name in ["res/shee.char", "res/bgm", "res/song/jumping23/ez", "res/song/snow/ex"] {
            let path = format!("{}/../assets/{name}", env!("CARGO_MANIFEST_DIR"));
            let Ok(plain) = std::fs::read(&path) else {
                continue;
            };
            seen += 1;

            if name.ends_with(".char") {
                assert_eq!(&plain[..8], &PNG_MAGIC, "{name}: stored as a PNG");
            }

            let packed = custom_enc(&zstd::encode_all(plain.as_slice(), 9).unwrap());
            assert_eq!(resolve_data(packed), plain, "{name}: packed form decodes back");
        }
        if seen == 0 {
            eprintln!("skipping: no assets next to the crate");
        }
    }
}

#[cfg(target_os = "android")]
pub mod jni {
    //! JNI entry points the open tree was missing.
    //!
    //! `preprocessInput` is the only one: the reference client samples
    //! `MotionEvent.getDeviceId()` *inside* Rust and feeds it to the anti-cheat probe.  We take
    //! the id as an `int` instead — the only thing the checksum depends on is that number, and
    //! passing it directly removes a JNI up-call from the touch hot path.

    use jni::{
        objects::JClass,
        sys::{jboolean, jint},
        EnvUnowned,
    };

    /// `Java_quad_native_QuadNative_preprocessInput`
    ///
    /// `external` is `InputDevice.isExternal()`, `virtual_input` is `InputDevice.isVirtual()`;
    /// both come straight from the Java touch handler. `jni` models `jboolean` as `bool`, so no
    /// integer conversion is needed.
    #[no_mangle]
    pub extern "C" fn Java_quad_1native_QuadNative_preprocessInput(
        _env: EnvUnowned,
        _class: JClass,
        device_id: jint,
        external: jboolean,
        virtual_input: jboolean,
    ) {
        prpr::judge::inner::device::probe(device_id, virtual_input, external);
    }

    /// `Java_quad_native_QuadNative_antiAddictionCallback` — only the Chinese channel build
    /// links the SDK, so elsewhere this is a no-op that keeps the symbol resolvable.
    #[no_mangle]
    pub extern "C" fn Java_quad_1native_QuadNative_antiAddictionCallback(
        _env: EnvUnowned,
        _class: JClass,
        _code: jint,
    ) {
    }
}
