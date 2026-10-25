//! Judge kernel.
//!
//! Upstream replaces [`super::JudgeInner`] with this module in `--cfg closed`
//! builds.  The public behaviour is identical to the open implementation —
//! `accuracy`, `score` and `counts` follow the same formulas — so the only
//! differences are internal:
//!
//! * every counter is mirrored in redundant encodings and re-verified on read
//!   (an anti-memory-edit measure; not reproduced here, see the module docs in
//!   `phira-inner-analysis.md` §5),
//! * hit offsets are accumulated as a running sum of squares so the record can
//!   carry the deviation, and
//! * a JNI probe records whether virtual/external input devices were seen; the
//!   result is baked into the score-upload token.

use super::{Judgement, PlayResult};

/// Running statistics of one play.
///
/// Kept deliberately close to the open-source [`super::JudgeInner`] so the two
/// can be swapped; the extra `diff_sq_sum` is what the closed build uploads.
pub struct JudgeInner {
    /// Offsets of `Good` hits, in commit order (used for the early/late split).
    diffs: Vec<f64>,

    combo: u32,
    max_combo: u32,
    counts: [u32; 4],
    num_of_notes: u32,
    early_kind: [u32; 4],
    late_kind: [u32; 4],

    /// `sum(effective_diff^2)` over every judgement.
    diff_sq_sum: f64,
}

/// Offset substituted for a `Miss`, which has no meaningful hit time.
///
/// The reference implementation uses a fixed quarter-second so a miss still
/// contributes a finite amount to the deviation.
const MISS_DIFF: f64 = 0.25;

impl JudgeInner {
    pub fn new(num_of_notes: u32) -> Self {
        Self {
            diffs: Vec::new(),
            combo: 0,
            max_combo: 0,
            counts: [0; 4],
            num_of_notes,
            early_kind: [0; 4],
            late_kind: [0; 4],
            diff_sq_sum: 0.,
        }
    }

    pub fn commit(&mut self, what: Judgement, diff: f64) {
        // `Miss` has no hit offset; substituting a fixed positive value keeps
        // the early/late split and the deviation well-defined.
        let effective = if matches!(what, Judgement::Miss) { MISS_DIFF } else { diff };

        if matches!(what, Judgement::Good) {
            self.diffs.push(diff);
        }
        if effective < 0. {
            self.early_kind[what as usize] += 1;
        } else if effective > 0. {
            self.late_kind[what as usize] += 1;
        }

        self.diff_sq_sum += effective * effective;
        self.counts[what as usize] += 1;

        match what {
            Judgement::Perfect | Judgement::Good => {
                self.combo += 1;
                if self.combo > self.max_combo {
                    self.max_combo = self.combo;
                }
            }
            _ => self.combo = 0,
        }
    }

    pub fn reset(&mut self) {
        self.combo = 0;
        self.max_combo = 0;
        self.counts = [0; 4];
        self.diffs.clear();
        self.early_kind = [0; 4];
        self.late_kind = [0; 4];
        self.diff_sq_sum = 0.;
    }

    pub fn accuracy(&self) -> f64 {
        (self.counts[0] as f64 + self.counts[1] as f64 * 0.65) / self.num_of_notes as f64
    }

    pub fn real_time_accuracy(&self) -> f64 {
        let cnt = self.counts.iter().sum::<u32>();
        if cnt == 0 {
            return 1.;
        }
        (self.counts[0] as f64 + self.counts[1] as f64 * 0.65) / cnt as f64
    }

    pub fn score(&self) -> u32 {
        const TOTAL: u32 = 1000000;
        if self.counts[0] == self.num_of_notes {
            TOTAL
        } else {
            let score = (0.9 * self.accuracy() + self.max_combo as f64 / self.num_of_notes as f64 * 0.1) * TOTAL as f64;
            score.round() as u32
        }
    }

    pub fn result(&self) -> PlayResult {
        let early = self.diffs.iter().filter(|it| **it < 0.).count() as u32;
        PlayResult {
            score: self.score(),
            accuracy: self.accuracy(),
            max_combo: self.max_combo,
            num_of_notes: self.num_of_notes,
            counts: self.counts,
            early,
            late: self.diffs.len() as u32 - early,
            std: self.deviation(),
            early_kind: self.early_kind,
            late_kind: self.late_kind,
        }
    }

    pub fn combo(&self) -> u32 {
        self.combo
    }

    pub fn counts(&self) -> [u32; 4] {
        self.counts
    }

    // -- values the score-upload record needs ------------------------------

    /// Highest combo reached, needed by the server to recompute [`Self::score`].
    pub fn max_combo(&self) -> u32 {
        self.max_combo
    }

    /// Total note count.  Also the divisor of [`Self::diff_variance`].
    pub fn num_of_notes(&self) -> u32 {
        self.num_of_notes
    }

    /// Variance of the hit offsets.  This — not the standard deviation — is what
    /// the upload record carries; the server takes the square root itself.
    pub fn diff_variance(&self) -> f64 {
        self.diff_sq_sum / self.num_of_notes as f64
    }

    /// Standard deviation of the hit offsets, as shown in the results screen.
    pub fn deviation(&self) -> f32 {
        self.diff_variance().sqrt() as f32
    }
}

/// Device attestation state, sampled by the JNI probe and embedded in the
/// score-upload record.
///
/// [`device::probe`] is a byte-for-byte transcription of
/// `judge__inner__jni_check_device` (`0xD92F5C`): the client does not send a raw
/// device id, it encodes it.
///
/// ```text
/// n    = MotionEvent.getDeviceId()      // int, the device that produced the touch
/// base = ((n ^ 0x3F) + 1) & 0x7FFF_FFFF
/// old  = atomic_fetch_and(fp, ~0x4000_0000)      // returns the previous fp
/// diff = old ^ base
/// fold = diff | diff>>8 | diff>>4 | diff>>2 | diff>>1
/// neg  = (old != 0 && base != 0) ? 3 : 0
/// salt = SALT[neg & fold & 3] ^ 0x32
/// fp   = base | (salt << 24)
///
/// SALT = 32 98 68 ea    ->   0x00 / 0xAA / 0x5A / 0xD8
/// ```
///
/// `fold` squashes any set bit of `diff` down into the low bits, so `fold & 3`
/// can only take three values:
///
/// | `diff` | index | salt | meaning |
/// |---|---|---|---|
/// | 0 | 0 | `0x00` | first probe, or the device did not change |
/// | 1 | 1 | `0xAA` | changed, old and new differ only in the lowest bit |
/// | >= 2 | 3 | `0xD8` | changed, larger difference |
///
/// Index 2 (`0x5A`) is unreachable: `fold & 2` implies `fold & 1`.  The `68` in
/// the table is dead data.
///
/// So `salt` is a "how the fingerprint changed" tag byte, not encryption.  `n = 5`
/// gives `base = 59` and a first-probe salt of 0, hence `59` — exactly what was
/// captured on device.  Subsequent probes of the same device stay at 59.
///
/// # Why the open build still sends `0`
///
/// Everything downstream of the device id is reproduced here, but the id itself
/// has to come from a `MotionEvent`, and this repository contains **no Java at
/// all** — the whole Android shell (activity, Gradle project, input plumbing) is
/// closed source; the documented build path is "compile `libphira.so`, drop it
/// into the official APK".  The `jni` dependency this crate declares for Android
/// existed for exactly that closed module.
///
/// So nobody in the open build can call [`device::probe`] with a real event id.
/// Rather than ship a JNI probe that nothing calls, the acquisition is left to
/// whoever owns an event source:
///
/// ```ignore
/// // from a MotionEvent, mirroring jni_probe_input_device:
/// let id      = env.call_method(event, jni_str!("getDeviceId"), jni_sig!("()I"), &[])?.i()?;
/// let device  = env.call_method_static(input_device_class, jni_str!("getDevice"),
///                                      jni_sig!("(I)Landroid/view/InputDevice;"), &[id.into()])?;
/// let virt    = env.call_method(&device, jni_str!("isVirtual"),  jni_sig!("()Z"), &[])?.z()?;
/// let ext     = env.call_method(&device, jni_str!("isExternal"), jni_sig!("()Z"), &[])?.z()?;
/// prpr::judge::inner::device::probe(id, virt, ext);
/// ```
///
/// Until that call exists, `fingerprint()` returns `0`, meaning "probe never
/// ran": the server can tell it apart, but it cannot be mistaken for tampering,
/// which is strictly safer than fabricating a number.  The two booleans still
/// come out right either way — a clean device sends `0xD8` / `'t'` / `0xD2`
/// regardless.
pub mod device {
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

    /// Encoded device fingerprint, or `0` when the probe never ran.
    static FINGERPRINT: AtomicU32 = AtomicU32::new(0);
    /// `true` if a virtual or external input device was observed.
    static VIRTUAL_INPUT: AtomicBool = AtomicBool::new(false);
    /// `true` if the probe detected a mismatch it considers tampering.
    static TAMPERED: AtomicBool = AtomicBool::new(false);

    pub fn fingerprint() -> u32 {
        FINGERPRINT.load(Ordering::Acquire)
    }

    pub fn virtual_input() -> bool {
        VIRTUAL_INPUT.load(Ordering::Acquire)
    }

    pub fn tampered() -> bool {
        TAMPERED.load(Ordering::Acquire)
    }

    /// Salt table at `byte_27393C` in the client.  Only the first four bytes are
    /// ever indexed; the trailing `00 01 00 01` is padding.
    const SALT: [u8; 4] = [0x32, 0x98, 0x68, 0xEA];
    /// `0x32`, so that index 0 yields a zero high byte.
    const SALT_XOR: u8 = 0x32;

    /// Record one probe and return the encoded fingerprint.
    ///
    /// This is the byte-for-byte transcription of `judge__inner__jni_check_device`
    /// (`0xD92F5C`).  `device_id` is `MotionEvent.getDeviceId()`, i.e. the id of
    /// the device that produced the touch — **not** an enumeration index.
    ///
    /// The value is a mixed base plus a state-dependent salt byte: two probes
    /// that agree leave it unchanged, but seeing a *different* device id sets the
    /// high byte (the client's "input source changed mid-session" signal).
    pub fn probe(device_id: i32, virtual_input: bool, external: bool) -> u32 {
        // base = ((n ^ 0x3F) + 1) & 0x7FFF_FFFF
        let base = ((device_id as u32) ^ 0x3F).wrapping_add(1) & 0x7FFF_FFFF;

        // The client does `atomic_fetch_and(fp, ~0x40000000)` and uses the old
        // value purely as "have we been set before?".  The masking itself never
        // matters, because the final store overwrites the whole word.
        let old = FINGERPRINT.fetch_and(!0x4000_0000u32, Ordering::AcqRel);
        let previously_set = old != 0 && base != 0;

        // `v34` is 0 or 1, used as `(-v34) & 3`, so the index is either 0 (never
        // set / base zero) or `3 & fold(old ^ base)`.
        let diff = old ^ base;
        let fold = diff | (diff >> 8) | (diff >> 4) | (diff >> 2) | (diff >> 1);
        let neg = if previously_set { 3 } else { 0 };
        let salt = SALT[(neg & fold & 3) as usize] ^ SALT_XOR;

        let fp = base | ((salt as u32) << 24);
        FINGERPRINT.store(fp, Ordering::Release);

        // Flag handling, also verbatim: a change against the *previous* value
        // raises the virtual flag, and any set bit raises the tampered flag.
        // Both globals are sticky, so in practice the first `true` wins.
        let old_virtual = VIRTUAL_INPUT.load(Ordering::Acquire);
        let old_tampered = TAMPERED.load(Ordering::Acquire);
        if (virtual_input ^ old_virtual) | (external ^ old_tampered) {
            VIRTUAL_INPUT.store(true, Ordering::Release);
        }
        if virtual_input | external {
            TAMPERED.store(true, Ordering::Release);
        }

        fp
    }

    /// Clear all attestation state.  Called when a new chart starts, so a flag
    /// raised during one play does not leak into the next — and so that the salt
    /// state machine restarts, which is what makes every play's first probe
    /// encode to the same value.
    pub fn reset() {
        FINGERPRINT.store(0, Ordering::Release);
        VIRTUAL_INPUT.store(false, Ordering::Release);
        TAMPERED.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::device;

    /// The device these globals describe is process-wide, so every test that
    /// touches them has to serialise.  `cargo test` runs a module's tests on
    /// several threads by default.
    fn serial() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        device::reset();
        guard
    }

    /// The one value observed on device: a clean phone whose touch event came
    /// from input device 5.
    #[test]
    fn a_clean_probe_reproduces_the_captured_fingerprint() {
        let _g = serial();
        assert_eq!(device::probe(5, false, false), 59);
        assert_eq!(device::fingerprint(), 59);
        assert!(!device::virtual_input());
        assert!(!device::tampered());
    }

    /// Repeating the same device must not move the value: `old == base` makes the
    /// fold zero, so the salt byte stays 0.  This is why all four probes captured
    /// on device reported 59.
    #[test]
    fn repeated_probes_of_the_same_device_are_stable() {
        let _g = serial();
        for _ in 0..4 {
            assert_eq!(device::probe(5, false, false), 59);
        }
    }

    /// A *different* device id is what raises the high byte: `old ^ base` becomes
    /// non-zero, the fold lands on index 1, and `0x98 ^ 0x32 == 0xAA`.
    #[test]
    fn changing_device_raises_the_salt_byte() {
        let _g = serial();
        assert_eq!(device::probe(5, false, false), 0x003B);
        assert_eq!(device::probe(6, false, false), 0xAA00_003A);
        assert_eq!(device::fingerprint(), 0xAA00_003A);
    }

    /// A virtual or external input device sets both sticky flags, which is what
    /// the record's `+28` / `+82` / `+83` low bits come from.
    #[test]
    fn a_virtual_device_marks_both_flags() {
        let _g = serial();
        device::probe(5, true, false);
        assert!(device::virtual_input());
        assert!(device::tampered());
    }

    /// The salt table has four entries but only three are reachable: `fold & 2`
    /// implies `fold & 1`, so index 2 (`0x5A`) can never be selected.  Sweeping
    /// small device ids covers `diff == 0`, `diff == 1` and `diff >= 2`.
    #[test]
    fn the_salt_never_lands_on_the_dead_table_entry() {
        let _g = serial();
        for prev in 0..8 {
            for next in 0..8 {
                device::reset();
                device::probe(prev, false, false);
                let salt = (device::probe(next, false, false) >> 24) as u8;
                assert!(
                    salt == 0x00 || salt == 0xAA || salt == 0xD8,
                    "prev={prev} next={next} produced salt {salt:#04x}"
                );
            }
        }
    }

    /// `reset()` must clear the fingerprint as well as the flags, otherwise the
    /// next play's first probe would carry another play's salt byte.
    #[test]
    fn reset_clears_the_salt_history() {
        let _g = serial();
        device::probe(5, true, true);
        device::reset();
        assert_eq!(device::fingerprint(), 0);
        assert!(!device::virtual_input());
        assert!(!device::tampered());
        assert_eq!(device::probe(5, false, false), 59);
    }
}
