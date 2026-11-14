use super::ConfigurationInterpolationError;
use crate::interpolation::Expansion;
use crate::selection::SelectionLimits;
use serde_json::{Map, Value};

pub(crate) fn expand(value: &str, variables: Option<&Map<String, Value>>, field: String) -> Result<String, ConfigurationInterpolationError> {
    let empty = Map::new();
    let limits = SelectionLimits::default();
    let mut expansion = Expansion::new(
        variables.unwrap_or(&empty),
        limits.variable_depth,
        limits.variable_nodes,
        limits.variable_scalar.checked_bytes().unwrap_or_default(),
    );
    expand_one(value, &mut expansion, field)
}
fn expand_one(value: &str, expansion: &mut Expansion<'_>, field: String) -> Result<String, ConfigurationInterpolationError> {
    match value.contains("{{") || value.contains("}}") {
        | false => Ok(value.to_string()),
        | true => expansion
            .expand(value)
            .map_err(|error| ConfigurationInterpolationError {
                field: field.clone(),
                reason: error.to_string(),
            })
            .and_then(|expanded| {
                if expanded.contains('\0') || expanded.contains("{{") || expanded.contains("}}") {
                    Err(ConfigurationInterpolationError {
                        field,
                        reason: "expanded value contains NUL or unresolved placeholder".to_string(),
                    })
                } else {
                    Ok(expanded)
                }
            }),
    }
}
pub(crate) fn expand_values(
    values: &[String],
    variables: Option<&Map<String, Value>>,
    field: &str,
) -> Result<Vec<String>, ConfigurationInterpolationError> {
    let empty = Map::new();
    let limits = SelectionLimits::default();
    let mut expansion = Expansion::new(
        variables.unwrap_or(&empty),
        limits.variable_depth,
        limits.variable_nodes,
        limits.variable_scalar.checked_bytes().unwrap_or_default(),
    );
    values
        .iter()
        .enumerate()
        .map(|(index, value)| expand_one(value, &mut expansion, format!("{field}[{index}]")))
        .collect()
}
