use super::alloc::collections::BTreeMap;
use super::Config;
use core::fmt;
use serde_json::{Map, Value};

/// An invalid invocation variable binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VariableOverrideError {
    /// A binding did not contain `=`.
    MissingEquals,
    /// A name did not match the invocation naming rule.
    InvalidName(String),
    /// A name belongs to the execution engine.
    ReservedName(String),
}
/// Validated string values supplied for one workflow invocation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VariableOverrides(BTreeMap<String, String>);
impl core::error::Error for VariableOverrideError {}
impl fmt::Display for VariableOverrideError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            | Self::MissingEquals => write!(formatter, "--var requires NAME=VALUE"),
            | Self::InvalidName(name) => write!(formatter, "invalid variable name {name:?}; expected [A-Z][A-Z0-9_]*"),
            | Self::ReservedName(name) if name == "OUTPUT_DIRECTORY" => {
                write!(formatter, "OUTPUT_DIRECTORY is managed; use --artifact-output")
            }
            | Self::ReservedName(name) => write!(formatter, "variable name {name} is managed by Xylem"),
        }
    }
}
impl VariableOverrides {
    /// Parse `NAME=VALUE` bindings, retaining the last occurrence of each name.
    ///
    /// Values remain strings and are never read from the host environment.
    pub fn parse(values: &[String]) -> Result<Self, VariableOverrideError> {
        values.iter().try_fold(Self::default(), |mut overrides, binding| {
            binding
                .split_once('=')
                .ok_or(VariableOverrideError::MissingEquals)
                .and_then(|(name, value)| {
                    validate_name(name).map(|()| {
                        overrides.0.insert(name.to_string(), value.to_string());
                        overrides
                    })
                })
        })
    }
    /// Apply bindings to the invocation's root workflow values.
    #[must_use]
    pub fn apply_root(&self, mut config: Config) -> Config {
        apply_values(&mut config.variables, &self.0);
        config
    }
    /// Reapply bindings to each resolved leaf so module defaults cannot shadow them.
    #[must_use]
    pub fn apply_resolved(&self, mut config: Config) -> Config {
        apply_values(&mut config.variables, &self.0);
        config.modules.iter_mut().for_each(|module| apply_values(&mut module.variables, &self.0));
        config
    }
    pub(super) fn with_named(mut self, named: &Self) -> Self {
        self.0.extend(named.0.iter().map(|(name, value)| (name.clone(), value.clone())));
        self
    }
    pub(super) fn apply_values(&self, values: &mut Option<Map<String, Value>>) {
        apply_values(values, &self.0);
    }
}
fn apply_values(target: &mut Option<Map<String, Value>>, overrides: &BTreeMap<String, String>) {
    if !overrides.is_empty() {
        let values = target.get_or_insert_default();
        values.extend(overrides.iter().map(|(name, value)| (name.clone(), Value::String(value.clone()))));
    }
}
pub(super) fn validate_name(name: &str) -> Result<(), VariableOverrideError> {
    let valid = name.as_bytes().split_first().is_some_and(|(first, rest)| {
        first.is_ascii_uppercase()
            && rest
                .iter()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || *byte == b'_')
    });
    match valid {
        | false => Err(VariableOverrideError::InvalidName(name.to_string())),
        | true if super::RESERVED_VARIABLES.contains(&name) || name == "OUTPUT_DIRECTORY" => {
            Err(VariableOverrideError::ReservedName(name.to_string()))
        }
        | true => Ok(()),
    }
}
