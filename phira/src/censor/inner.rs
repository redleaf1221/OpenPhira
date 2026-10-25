//! Startup preload of the banned-word list.
//!
//! Upstream keeps this in a proprietary module; the open build only needs the
//! endpoint.  Everything else — fetch, base64, `daachorse` automaton, on-disk
//! cache — already lives in [`super`], in the clear.
//!
//! # The shipping client never fetches anything
//!
//! The URL did **not** survive static analysis, and in the end it turned out not
//! to need to: the closed `preload` is an **empty future**.  The evidence, so
//! nobody repeats the search:
//!
//! * A raw byte scan of `libphira.so` finds no `/censor`, `/words` or any other
//!   path fragment that could be appended to the API base; the only `/censor`
//!   string in the image is `/censor.bin`, which is
//!   [`super::CensorManager::cache_path`]'s file name, not a route.
//! * `preload` (0x7DEF44) references exactly twenty `.rodata` addresses — all of
//!   them log messages — and never calls the API URL builder
//!   (`format!("{API_URL}{path}")`, at 0xA4BBC4).
//! * `GET https://phira.5wyxi.com/` answers 404, so the bare API base is not the
//!   endpoint either.
//! * Dynamically (`dyn/preload_url.js`): the poll is entered **once**, and
//!   `onLeave` sees `ret = 0`, i.e. `Poll::Ready`, with the state byte going
//!   0 → 1.  It completes on the first poll without doing any work.
//! * Consequently `censor.bin` is never created (not even after the whole cache
//!   directory was deleted), no request is ever opened, and not one
//!   `phira::censor` line reaches `logcat` — despite `EnvFilter` being `debug`.
//!
//! So the banned-word feature is dead in the build that ships.  A placeholder
//! URL here is therefore *behaviourally identical* to the closed build, and
//! nothing about this module is a gap in the "fully open source" goal.

use tracing::warn;

use super::CensorManager;

/// Word-list endpoint.
///
/// Placeholder.  The shipping client never gets this far — see the module docs.
/// Replace it if you run a private server and want the feature to actually work.
const WORD_LIST_URL: &str = "https://phira.5wyxi.com/censor/words";

/// Fetch (or load from cache) the word list and build the automaton.
///
/// Called once at startup.  [`CensorManager::init`] deduplicates concurrent
/// callers and leaves the cell empty on failure, so a later retry works.
pub async fn preload() {
    if let Err(e) = CensorManager::init(WORD_LIST_URL.to_owned()).await {
        warn!("failed to preload censor manager: {e:#}");
    }
}
