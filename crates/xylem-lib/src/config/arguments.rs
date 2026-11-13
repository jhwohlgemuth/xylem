use super::alloc::collections::BTreeSet;
use super::overrides::validate_name;
use super::{Config, VariableOverrideError, VariableOverrides};
use core::fmt;

/// A malformed positional workflow argument declaration or invocation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WorkflowArgumentsError {
    /// A declaration uses an invalid or engine-owned name.
    InvalidDeclaration(VariableOverrideError),
    /// A name is declared more than once.
    DuplicateDeclaration(String),
    /// More values were supplied than the workflow declares.
    TooManyValues { supplied: usize, declared: usize },
}
impl Config {
    /// Bind positional strings in root declaration order, then apply named bindings.
    ///
    /// Omitted trailing values do not override configured defaults.
    /// An empty string is an explicit binding. Nested configuration declarations do not participate.
    pub fn bind_arguments(&self, positional: &[String], named: &VariableOverrides) -> Result<VariableOverrides, WorkflowArgumentsError> {
        let declarations = self.arguments.as_deref().unwrap_or_default();
        validate_declarations(declarations).and_then(|()| match positional.len() > declarations.len() {
            | true => Err(WorkflowArgumentsError::TooManyValues {
                supplied: positional.len(),
                declared: declarations.len(),
            }),
            | false => {
                let bindings = declarations
                    .iter()
                    .zip(positional)
                    .map(|(name, value)| format!("{name}={value}"))
                    .collect::<Vec<_>>();
                VariableOverrides::parse(&bindings)
                    .map(|values| values.with_named(named))
                    .map_err(WorkflowArgumentsError::InvalidDeclaration)
            }
        })
    }
}
impl core::error::Error for WorkflowArgumentsError {}
impl fmt::Display for WorkflowArgumentsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            | Self::InvalidDeclaration(why) => write!(formatter, "invalid workflow argument declaration: {why}"),
            | Self::DuplicateDeclaration(name) => write!(formatter, "duplicate workflow argument declaration {name}"),
            | Self::TooManyValues { supplied, declared } => {
                write!(formatter, "received {supplied} positional values but workflow declares {declared}")
            }
        }
    }
}
pub(super) fn validate_declarations(declarations: &[String]) -> Result<(), WorkflowArgumentsError> {
    declarations
        .iter()
        .try_fold(BTreeSet::new(), |mut names, name| {
            validate_name(name)
                .map_err(WorkflowArgumentsError::InvalidDeclaration)
                .and_then(|()| match names.insert(name) {
                    | true => Ok(names),
                    | false => Err(WorkflowArgumentsError::DuplicateDeclaration(name.clone())),
                })
        })
        .map(|_| ())
}
