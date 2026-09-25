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
