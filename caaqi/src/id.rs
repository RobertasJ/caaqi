//! [`Id`], identifiers unique across the whole process.

use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

/// An identifier from a global atomic counter: every [`Id::new`] in the
/// process returns a different one, whichever context it's for. Ids are
/// never reused; `new` panics once the counter runs out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Id(u64);

impl Id {
    pub fn new() -> Id {
        Id(NEXT_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
            .expect("ID overflow"))
    }
}
