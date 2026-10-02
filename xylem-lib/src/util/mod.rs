use std::fs::canonicalize;
use std::path::Path;

pub mod constants;

/// Render a canonical path when it exists, otherwise preserve the supplied path.
pub(crate) fn absolute_path(path: impl AsRef<Path>) -> String {
    let path = path.as_ref();
    let value = canonicalize(path).unwrap_or_else(|_| path.to_path_buf()).display().to_string();
    #[cfg(windows)]
    let value = value.strip_prefix(r"\\?\").unwrap_or(&value).to_string();
    value
}
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

#[cfg(test)]
mod tests {
    use super::absolute_path;
    use std::path::Path;

    #[test]
    fn absolute_path_preserves_a_missing_path() {
        let path = Path::new("missing-xylem-path");

        assert_eq!(absolute_path(path), path.display().to_string());
    }
}
