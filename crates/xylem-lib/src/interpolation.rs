//! Bounded scalar interpolation for structural workflow fields.
use serde_json::{Map, Value};

#[derive(Debug, thiserror::Error)]
pub(crate) enum InterpolationError {
    #[error("{0}")]
    Invalid(String),
    #[error("expansion {resource} limit exceeded: {actual} > {limit}")]
    Limit { resource: &'static str, actual: u64, limit: u64 },
}
pub(crate) struct Expansion<'a> {
    variables: &'a Map<String, Value>,
    depth_limit: usize,
    substitution_limit: usize,
    byte_limit: u64,
    substitutions: usize,
}
impl<'a> Expansion<'a> {
    pub(crate) fn new(variables: &'a Map<String, Value>, depth_limit: usize, substitution_limit: usize, byte_limit: u64) -> Self {
        Self {
            variables,
            depth_limit,
            substitution_limit,
            byte_limit,
            substitutions: 0,
        }
    }
    pub(crate) fn expand(&mut self, value: &str) -> Result<String, InterpolationError> {
        self.expand_with_ancestors(value, &[])
    }
    fn append(&self, output: &mut String, value: &str) -> Result<(), InterpolationError> {
        let actual = output.len().saturating_add(value.len()) as u64;
        if actual > self.byte_limit {
            Err(InterpolationError::Limit {
                resource: "bytes",
                actual,
                limit: self.byte_limit,
            })
        } else {
            output.push_str(value);
            Ok(())
        }
    }
    fn expand_with_ancestors(&mut self, value: &str, ancestors: &[&str]) -> Result<String, InterpolationError> {
        let mut output = String::new();
        let mut remaining = Some(value);
        let mut result = Ok(());
        while let Some(fragment) = remaining.take() {
            result = match fragment.split_once("{{") {
                | None => self.append(&mut output, fragment),
                | Some((literal, template)) => self.append(&mut output, literal).and_then(|()| {
                    self.expand_placeholder(template, ancestors)
                        .and_then(|(expanded, suffix)| self.append(&mut output, &expanded).map(|()| remaining = Some(suffix)))
                }),
            };
        }
        result.map(|()| output)
    }
    fn expand_placeholder<'value>(&mut self, template: &'value str, ancestors: &[&str]) -> Result<(String, &'value str), InterpolationError> {
        template
            .split_once("}}")
            .ok_or_else(|| InterpolationError::Invalid("unclosed variable placeholder".to_string()))
            .and_then(|(name, suffix)| {
                let name = name.trim();
                self.substitutions = self.substitutions.saturating_add(1);
                match (
                    ancestors.contains(&name),
                    ancestors.len() >= self.depth_limit,
                    self.substitutions > self.substitution_limit,
                ) {
                    | (true, _, _) => Err(InterpolationError::Invalid(format!("cyclic variable reference {name}"))),
                    | (_, true, _) => Err(InterpolationError::Limit {
                        resource: "depth",
                        actual: ancestors.len().saturating_add(1) as u64,
                        limit: self.depth_limit as u64,
                    }),
                    | (_, _, true) => Err(InterpolationError::Limit {
                        resource: "substitutions",
                        actual: self.substitutions as u64,
                        limit: self.substitution_limit as u64,
                    }),
                    | (false, false, false) => {
                        let stack = ancestors.iter().copied().chain(core::iter::once(name)).collect::<Vec<_>>();
                        match self.variables.get(name) {
                            | Some(Value::String(value)) => self.expand_with_ancestors(value, &stack),
                            | Some(value @ (Value::Bool(_) | Value::Number(_))) => Ok(value.to_string()),
                            | Some(_) => Err(InterpolationError::Invalid(format!(
                                "variable {name} must be a string, number, or boolean"
                            ))),
                            | None => Err(InterpolationError::Invalid(format!("unresolved workflow variable {name}"))),
                        }
                    }
                }
                .map(|expanded| (expanded, suffix))
            })
    }
}
