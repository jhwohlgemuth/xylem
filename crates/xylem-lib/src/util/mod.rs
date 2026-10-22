use acorn_core::time::{Milliseconds, UnitOfTime};
use std::time::{SystemTime, UNIX_EPOCH};

pub mod constants;

/// Preserve first-seen order while removing duplicate values.
pub fn deduplicate<T>(values: &[T]) -> Vec<T>
where
    T: Clone + PartialEq,
{
    values.iter().fold(Vec::new(), |mut unique, value| {
        if !unique.contains(value) {
            unique.push(value.clone());
        }
        unique
    })
}
/// Return the current Unix timestamp in milliseconds.
pub(crate) fn timestamp() -> Milliseconds {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or_else(
        |_| Milliseconds::default(),
        |duration| Milliseconds::new(u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)),
    )
}
