//! Bounded workflow-variable expansion for selection roots before discovery.
//!
//! For example, `{{ DATA }}/tiles` becomes `inputs/tiles` with `DATA: "inputs"`.
//! References may be nested: `DATA: "{{ BASE }}/tiles"` and `BASE: "inputs"` give the same result for `{{ DATA }}`.
//! Scalar values also work: `YEAR: 2026` and `READY: true` expand `{{ YEAR }}/{{ READY }}` to `2026/true`.
//!
//! Bindings remain unchanged for CEL evaluation. Unknown names, cycles,
//! malformed placeholders, and non-scalar references fail before discovery;
//! depth, substitution count, and string-size limits bound expansion work.
use super::{SelectionError, SelectionLimits, SelectionOptions};
use serde_json::{Map, Value};

fn append(output: &mut String, value: &str, limits: &SelectionLimits) -> Result<(), SelectionError> {
    let actual = output.len().saturating_add(value.len()) as u64;
    let limit = limits.variable_scalar.checked_bytes().unwrap_or_default();
    match actual > limit {
        | true => Err(SelectionError::Limit {
            resource: "root_expansion_bytes",
            limit,
            actual,
        }),
        | false => {
            output.push_str(value);
            Ok(())
        }
    }
}
pub(super) fn expand(
    options: &SelectionOptions,
    variables: &Map<String, Value>,
    limits: &SelectionLimits,
) -> Result<SelectionOptions, SelectionError> {
    let mut substitutions = 0usize;
    options
        .roots
        .iter()
        .enumerate()
        .map(|(index, root)| {
            expand_value(root, variables, &[], &mut substitutions, limits)
                .and_then(
                    |expanded| match expanded.trim().is_empty() || expanded.contains('\0') || expanded.contains("{{") {
                        | true => Err(SelectionError::Request(
                            "expanded root must be nonempty and contain no NUL or unresolved placeholder".to_string(),
                        )),
                        | false => Ok(expanded),
                    },
                )
                .map_err(|error| match error {
                    | SelectionError::Request(reason) => SelectionError::Request(format!("options.roots[{index}]: {reason}")),
                    | other => other,
                })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|roots| SelectionOptions {
            recursive: options.recursive,
            roots,
        })
}
fn expand_value(
    value: &str,
    variables: &Map<String, Value>,
    ancestors: &[&str],
    substitutions: &mut usize,
    limits: &SelectionLimits,
) -> Result<String, SelectionError> {
    let mut output = String::new();
    let mut remaining = value;
    while let Some((literal, template)) = remaining.split_once("{{") {
        let next = append(&mut output, literal, limits).and_then(|()| {
            template
                .split_once("}}")
                .ok_or_else(|| SelectionError::Request("unclosed variable placeholder".to_string()))
                .and_then(|(name, suffix)| {
                    let name = name.trim();
                    *substitutions = substitutions.saturating_add(1);
                    let depth_exceeded = ancestors.len() >= limits.variable_depth;
                    let substitutions_exceeded = *substitutions > limits.variable_nodes;
                    match (ancestors.contains(&name), depth_exceeded, substitutions_exceeded) {
                        | (true, _, _) => Err(SelectionError::Request(format!("cyclic variable reference {name}"))),
                        | (_, true, _) => Err(SelectionError::Limit {
                            resource: "root_expansion_depth",
                            limit: limits.variable_depth as u64,
                            actual: ancestors.len().saturating_add(1) as u64,
                        }),
                        | (_, _, true) => Err(SelectionError::Limit {
                            resource: "root_expansion_substitutions",
                            limit: limits.variable_nodes as u64,
                            actual: *substitutions as u64,
                        }),
                        | (false, false, false) => {
                            let stack = ancestors.iter().copied().chain(core::iter::once(name)).collect::<Vec<_>>();
                            match variables.get(name) {
                                | Some(Value::String(value)) => expand_value(value, variables, &stack, substitutions, limits),
                                | Some(value @ (Value::Bool(_) | Value::Number(_))) => Ok(value.to_string()),
                                | Some(_) => Err(SelectionError::Request(format!("variable {name} must be a string, number, or boolean"))),
                                | None => Err(SelectionError::Request(format!("unresolved workflow variable {name}"))),
                            }
                        }
                    }
                    .and_then(|expanded| append(&mut output, &expanded, limits))
                    .map(|()| suffix)
                })
        });
        match next {
            | Ok(suffix) => remaining = suffix,
            | Err(error) => return Err(error),
        }
    }
    append(&mut output, remaining, limits).map(|()| output)
}
