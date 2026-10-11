//! Constants shared by Xylem runtime modules.
/// Canonical application identity.
pub mod app;

/// Matches an upper-snake-case Handlebars-style variable placeholder.
pub(crate) const RE_HANDLEBARS: &str = r"\{\{\s*(?P<variable>[A-Z][A-Z0-9_]*)\s*\}\}";

/// Matches optional padding inside an upper-snake-case variable placeholder.
pub(crate) const RE_TEMPLATE_SPACES: &str = r"((?<=\{\{) *(?=[A-Z])|(?<=[A-Z0-9_]) *(?=\}\}))";
