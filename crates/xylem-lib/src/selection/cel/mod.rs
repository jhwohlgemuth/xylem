extern crate alloc;

use super::{
    InputSource, ResolvedInputs, ResolvedSelectionOptions, ScriptingEngine, ScriptingEngineMetadata, SelectionError, SelectionItem, SelectionLimits,
    SelectionOptions, SelectionRequest,
};
use acorn_schema::hardware::Memory;
use alloc::collections::{BTreeMap, BTreeSet};
use cel::common::ast::{EntryExpr, Expr, IdedExpr, LiteralValue};
use cel::{Context, Program, Value as CelValue};
use core::cell::Cell;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::fs::{canonicalize, read_dir, symlink_metadata, DirEntry};
use std::path::{Path, PathBuf};

const CEL_INTERPRETER: &str = "cel-rust";
const CEL_INTERPRETER_VERSION: &str = "0.14.5";
const CEL_LANGUAGE: &str = "CEL";
const CEL_LANGUAGE_VERSION: &str = "selection-subset-v1";
const INVENTORY_DIGEST_VERSION: &[u8] = b"xylem-selection-inventory-v1";
trait ExpressionExt {
    fn appended_expression(&self) -> Option<&IdedExpr>;
    fn estimate(
        &self,
        scope: &BTreeMap<String, Estimate>,
        inventory: &Inventory,
        variables: &Map<String, Value>,
        limits: &SelectionLimits,
    ) -> Result<Estimate, SelectionError>;
    #[allow(clippy::too_many_arguments)]
    fn estimate_comprehension(
        &self,
        iter_var: &str,
        range: Estimate,
        initial: Estimate,
        condition: Estimate,
        step: Estimate,
        result: Estimate,
        scope: &BTreeMap<String, Estimate>,
        inventory: &Inventory,
        variables: &Map<String, Value>,
        limits: &SelectionLimits,
    ) -> Result<Estimate, SelectionError>;
    fn lookup<'a>(&self, variables: &'a Map<String, Value>) -> Option<&'a Value>;
}
trait Validate<ValidationContext> {
    fn validate(&self, context: ValidationContext) -> Result<(), SelectionError>;
}
#[derive(Clone, Debug, Eq, PartialEq)]
enum Function {
    Add,
    And,
    Conditional,
    Contains,
    Divide,
    Double,
    EndsWith,
    Equal,
    GreaterThan,
    GreaterThanOrEqual,
    In,
    Index,
    Int,
    LessThan,
    LessThanOrEqual,
    Modulo,
    Multiply,
    Negate,
    Not,
    NotEqual,
    NotStrictlyFalse,
    Or,
    Size,
    StartsWith,
    String,
    Subtract,
    UInt,
    Unsupported(String),
}
#[derive(Clone, Debug, Eq, PartialEq)]
enum ValueKind {
    Bool,
    Item,
    ItemString,
    List(Box<ValueKind>),
    Map,
    Null,
    Number,
    Options,
    String,
    Unknown,
    Variables,
}
#[derive(Clone, Debug, Default)]
struct CandidateProfile {
    inventory_bytes: usize,
    max_extension_bytes: usize,
    max_name_bytes: usize,
    max_path_bytes: usize,
    max_relative_path_bytes: usize,
}

pub(super) struct Cel;
#[derive(Debug, Default)]
struct DiscoveryAccumulator {
    items: Vec<SelectionItem>,
    metadata_operations: usize,
    path_bytes: usize,
    profile: CandidateProfile,
    seen: BTreeSet<String>,
    skipped_unsupported_entries: usize,
    visited_entries: usize,
}
#[derive(Clone, Debug)]
struct Estimate {
    bytes: u64,
    cost: u64,
    kind: ValueKind,
    length: u64,
    records: u64,
}
#[derive(Clone, Copy)]
struct ExpressionValidation<'a> {
    depth: usize,
    limits: &'a SelectionLimits,
    nodes: &'a Cell<usize>,
    scope: &'a BTreeSet<String>,
}
#[derive(Debug)]
pub(super) struct Inventory {
    digest: String,
    items: Vec<SelectionItem>,
    options: ResolvedSelectionOptions,
    profile: CandidateProfile,
    skipped_unsupported_entries: usize,
}
#[derive(Default)]
struct VariableCounter {
    bytes: usize,
    nodes: usize,
}
impl ScriptingEngine for Cel {
    type CompiledScript = Program;
    type Context = Inventory;

    fn compile(
        &self,
        script: &str,
        inventory: &Self::Context,
        variables: &Map<String, Value>,
        limits: &SelectionLimits,
    ) -> Result<Self::CompiledScript, SelectionError> {
        validate_variable_bindings(variables, limits).and_then(|()| {
            Program::compile(script)
                .map_err(|error| SelectionError::Parse(error.to_string()))
                .and_then(|program| {
                    let nodes = Cell::new(0);
                    let scope = ["items", "options", "variables"].into_iter().map(str::to_string).collect::<BTreeSet<_>>();
                    program
                        .expression()
                        .validate(ExpressionValidation {
                            depth: 1,
                            limits,
                            nodes: &nodes,
                            scope: &scope,
                        })
                        .and_then(|()| {
                            let root_scope = inventory.root_estimates(variables);
                            program.expression().estimate(&root_scope, inventory, variables, limits).map(|_| program)
                        })
                })
        })
    }
    fn evaluate(
        &self,
        script: &Self::CompiledScript,
        inventory: &Self::Context,
        variables: &Map<String, Value>,
        limits: &SelectionLimits,
    ) -> Result<Vec<Option<String>>, SelectionError> {
        evaluate(script, inventory, variables, limits)
    }
    fn metadata(&self) -> ScriptingEngineMetadata {
        ScriptingEngineMetadata {
            interpreter: CEL_INTERPRETER,
            interpreter_version: CEL_INTERPRETER_VERSION,
            language: CEL_LANGUAGE,
            language_version: CEL_LANGUAGE_VERSION,
        }
    }
}
impl DiscoveryAccumulator {
    fn visit_directory(
        self,
        root_index: usize,
        directory: &Path,
        relative_parent: &str,
        parent_depth: usize,
        recursive: bool,
        limits: &SelectionLimits,
    ) -> Result<Self, SelectionError> {
        read_entries(directory, self.visited_entries, limits.visited_entries).and_then(|entries| {
            entries.into_iter().try_fold(self, |state, (name, entry)| {
                let relative_path = match relative_parent.is_empty() {
                    | true => name,
                    | false => format!("{relative_parent}/{name}"),
                };
                inspect_entry(state, root_index, entry, relative_path, parent_depth.saturating_add(1), recursive, limits)
            })
        })
    }
    fn add_candidate(mut self, item: SelectionItem, limits: &SelectionLimits) -> Result<Self, SelectionError> {
        match self.seen.insert(item.path.clone()) {
            | false => Ok(self),
            | true => {
                let path_bytes = item.path.len().saturating_add(item.relative_path.len());
                let next_path_bytes = self.path_bytes.saturating_add(path_bytes);
                check_memory_limit("path", next_path_bytes as u64, &limits.path)
                    .and_then(|()| check_limit("candidates", self.items.len().saturating_add(1), limits.candidates))
                    .map(|()| {
                        self.path_bytes = next_path_bytes;
                        self.profile.inventory_bytes = self
                            .profile
                            .inventory_bytes
                            .saturating_add(path_bytes)
                            .saturating_add(item.name.len())
                            .saturating_add(item.extension.len())
                            .saturating_add(item.kind.len())
                            .saturating_add(32);
                        self.profile.max_extension_bytes = self.profile.max_extension_bytes.max(item.extension.len());
                        self.profile.max_name_bytes = self.profile.max_name_bytes.max(item.name.len());
                        self.profile.max_path_bytes = self.profile.max_path_bytes.max(item.path.len());
                        self.profile.max_relative_path_bytes = self.profile.max_relative_path_bytes.max(item.relative_path.len());
                        self.items.push(item);
                        self
                    })
            }
        }
    }
}
impl Estimate {
    fn add(values: Vec<Self>, cost: u64) -> Result<Self, SelectionError> {
        match values.as_slice() {
            | [left, right] => match (&left.kind, &right.kind) {
                | (ValueKind::List(left_kind), ValueKind::List(right_kind)) => Ok(Self {
                    bytes: left.bytes.saturating_add(right.bytes),
                    cost,
                    kind: ValueKind::List(Box::new(match left_kind == right_kind {
                        | true => left_kind.as_ref().clone(),
                        | false => ValueKind::Unknown,
                    })),
                    length: left.length.saturating_add(right.length),
                    records: left.records.saturating_add(right.records),
                }),
                | (ValueKind::String | ValueKind::ItemString, ValueKind::String | ValueKind::ItemString) => Ok(Self {
                    bytes: left.bytes.saturating_add(right.bytes),
                    cost,
                    kind: ValueKind::String,
                    length: 0,
                    records: 1,
                }),
                | (ValueKind::Number, ValueKind::Number) => Ok(Self {
                    cost,
                    ..Self::scalar(ValueKind::Number, 16)
                }),
                | _ => Err(SelectionError::Check("cannot statically bound addition operands".to_string())),
            },
            | _ => Err(SelectionError::Check("addition requires two operands".to_string())),
        }
    }
    fn enforce(self, limits: &SelectionLimits) -> Result<Self, SelectionError> {
        check_limit_u64("evaluation_cost", self.cost, limits.evaluation_cost)
            .and_then(|()| check_limit_u64("intermediate_records", self.records, limits.intermediate_records as u64))
            .and_then(|()| check_memory_limit("intermediate", self.bytes, &limits.intermediate))
            .map(|()| self)
    }
    fn json_value(value: &Value) -> Self {
        match value {
            | Value::Array(values) => values.iter().fold(
                Self {
                    bytes: 0,
                    cost: 1,
                    kind: ValueKind::List(Box::new(ValueKind::Unknown)),
                    length: values.len() as u64,
                    records: 0,
                },
                |mut estimate, value| {
                    let value = Self::json_value(value);
                    estimate.bytes = estimate.bytes.saturating_add(value.bytes);
                    estimate.records = estimate.records.saturating_add(value.records);
                    estimate.kind = match estimate.kind {
                        | ValueKind::List(current) if *current == ValueKind::Unknown || *current == value.kind => {
                            ValueKind::List(Box::new(value.kind))
                        }
                        | ValueKind::List(_) => ValueKind::List(Box::new(ValueKind::Unknown)),
                        | kind => kind,
                    };
                    estimate
                },
            ),
            | Value::Bool(_) => Self::scalar(ValueKind::Bool, 1),
            | Value::Null => Self::scalar(ValueKind::Null, 0),
            | Value::Number(_) => Self::scalar(ValueKind::Number, 16),
            | Value::Object(values) => values.iter().fold(
                Self {
                    bytes: 0,
                    cost: 1,
                    kind: ValueKind::Map,
                    length: values.len() as u64,
                    records: values.len() as u64,
                },
                |mut estimate, (key, value)| {
                    let value = Self::json_value(value);
                    estimate.bytes = estimate.bytes.saturating_add(key.len() as u64).saturating_add(value.bytes);
                    estimate.records = estimate.records.saturating_add(value.records);
                    estimate
                },
            ),
            | Value::String(value) => Self::scalar(ValueKind::String, value.len()),
        }
    }
    fn literal(literal: &LiteralValue) -> Self {
        match literal {
            | LiteralValue::Boolean(_) => Self::scalar(ValueKind::Bool, 1),
            | LiteralValue::Bytes(value) => Self::scalar(ValueKind::Unknown, value.inner().len()),
            | LiteralValue::Double(_) | LiteralValue::Int(_) | LiteralValue::UInt(_) => Self::scalar(ValueKind::Number, 16),
            | LiteralValue::Null => Self::scalar(ValueKind::Null, 0),
            | LiteralValue::String(value) => Self::scalar(ValueKind::String, value.inner().len()),
        }
    }
    fn scalar(kind: ValueKind, bytes: usize) -> Self {
        Self {
            bytes: bytes as u64,
            cost: 1,
            kind,
            length: 0,
            records: 1,
        }
    }
    fn select(self, expression: &IdedExpr, field: &str, inventory: &Inventory, variables: &Map<String, Value>) -> Self {
        match expression.lookup(variables) {
            | Some(value) => {
                let mut estimate = Self::json_value(value);
                estimate.cost = self.cost.saturating_add(1);
                estimate
            }
            | None => {
                let (kind, bytes, records, length) = match self.kind {
                    | ValueKind::Item => match field {
                        | "extension" => (ValueKind::ItemString, inventory.profile.max_extension_bytes as u64, 1, 0),
                        | "kind" => (ValueKind::ItemString, 9, 1, 0),
                        | "name" => (ValueKind::ItemString, inventory.profile.max_name_bytes as u64, 1, 0),
                        | "path" => (ValueKind::ItemString, inventory.profile.max_path_bytes as u64, 1, 0),
                        | "relative_path" => (ValueKind::ItemString, inventory.profile.max_relative_path_bytes as u64, 1, 0),
                        | "depth" | "index" | "size" => (ValueKind::Number, 16, 1, 0),
                        | _ => (ValueKind::Unknown, self.bytes, self.records, self.length),
                    },
                    | ValueKind::Options => match field {
                        | "recursive" => (ValueKind::Bool, 1, 1, 0),
                        | "roots" => (
                            ValueKind::List(Box::new(ValueKind::String)),
                            self.bytes.saturating_sub(1),
                            self.records.saturating_sub(1),
                            self.records.saturating_sub(1),
                        ),
                        | _ => (ValueKind::Unknown, self.bytes, self.records, self.length),
                    },
                    | _ => (ValueKind::Unknown, self.bytes, self.records, self.length),
                };
                Self {
                    bytes,
                    cost: self.cost.saturating_add(1),
                    kind,
                    length,
                    records,
                }
            }
        }
    }
}
impl From<&str> for Function {
    fn from(name: &str) -> Self {
        match name {
            | "_+_" => Self::Add,
            | "_&&_" => Self::And,
            | "_?_:_" => Self::Conditional,
            | "contains" => Self::Contains,
            | "_/_" => Self::Divide,
            | "double" => Self::Double,
            | "endsWith" => Self::EndsWith,
            | "_==_" => Self::Equal,
            | "_>_" => Self::GreaterThan,
            | "_>=_" => Self::GreaterThanOrEqual,
            | "@in" => Self::In,
            | "_[_]" => Self::Index,
            | "int" => Self::Int,
            | "_<_" => Self::LessThan,
            | "_<=_" => Self::LessThanOrEqual,
            | "_%_" => Self::Modulo,
            | "_*_" => Self::Multiply,
            | "-_" => Self::Negate,
            | "!_" => Self::Not,
            | "_!=_" => Self::NotEqual,
            | "@not_strictly_false" => Self::NotStrictlyFalse,
            | "_||_" => Self::Or,
            | "size" => Self::Size,
            | "startsWith" => Self::StartsWith,
            | "string" => Self::String,
            | "_-_" => Self::Subtract,
            | "uint" => Self::UInt,
            | _ => Self::Unsupported(name.to_string()),
        }
    }
}
impl Function {
    fn estimate(&self, target: Option<Estimate>, arguments: Vec<Estimate>) -> Result<Estimate, SelectionError> {
        let mut values = target.into_iter().chain(arguments).collect::<Vec<_>>();
        let cost = values.iter().fold(1u64, |cost, value| cost.saturating_add(value.cost));
        let bytes = values.iter().fold(0u64, |bytes, value| bytes.saturating_add(value.bytes));
        match self {
            | Self::And
            | Self::Contains
            | Self::EndsWith
            | Self::Equal
            | Self::GreaterThan
            | Self::GreaterThanOrEqual
            | Self::In
            | Self::LessThan
            | Self::LessThanOrEqual
            | Self::Not
            | Self::NotEqual
            | Self::NotStrictlyFalse
            | Self::Or
            | Self::StartsWith => Ok(Estimate {
                bytes: 1,
                cost: cost.saturating_add(bytes / 8),
                kind: ValueKind::Bool,
                length: 0,
                records: 1,
            }),
            | Self::Double | Self::Int | Self::Size | Self::UInt | Self::Divide | Self::Modulo | Self::Multiply | Self::Negate | Self::Subtract => {
                Ok(Estimate {
                    bytes: 16,
                    cost,
                    kind: ValueKind::Number,
                    length: 0,
                    records: 1,
                })
            }
            | Self::String => Ok(Estimate {
                bytes: values.first().map_or(64, |value| value.bytes.max(64)),
                cost,
                kind: ValueKind::String,
                length: 0,
                records: 1,
            }),
            | Self::Conditional => {
                let branches = values.split_off(values.len().min(1));
                Ok(branches.into_iter().fold(Estimate::scalar(ValueKind::Unknown, 0), |mut result, branch| {
                    result.bytes = result.bytes.max(branch.bytes);
                    result.cost = cost;
                    result.kind = match result.kind == ValueKind::Unknown || result.kind == branch.kind {
                        | true => branch.kind,
                        | false => ValueKind::Unknown,
                    };
                    result.length = result.length.max(branch.length);
                    result.records = result.records.max(branch.records);
                    result
                }))
            }
            | Self::Index => Ok(values.first().map_or_else(
                || Estimate::scalar(ValueKind::Unknown, 0),
                |value| match &value.kind {
                    | ValueKind::List(element) => Estimate {
                        kind: element.as_ref().clone(),
                        cost,
                        ..Estimate::scalar(ValueKind::Unknown, value.bytes as usize)
                    },
                    | _ => Estimate {
                        cost,
                        ..Estimate::scalar(ValueKind::Unknown, value.bytes as usize)
                    },
                },
            )),
            | Self::Add => Estimate::add(values, cost),
            | Self::Unsupported(name) => Err(SelectionError::Check(format!("cannot estimate function {name}"))),
        }
    }
}
impl Validate<()> for Function {
    fn validate(&self, (): ()) -> Result<(), SelectionError> {
        match self {
            | Self::Unsupported(name) => Err(SelectionError::Check(format!("unsupported function or operator {name}"))),
            | _ => Ok(()),
        }
    }
}
impl ExpressionExt for IdedExpr {
    fn appended_expression(&self) -> Option<&IdedExpr> {
        match &self.expr {
            | Expr::List(list) => list.elements.first(),
            | Expr::Call(call) => call
                .args
                .iter()
                .find_map(ExpressionExt::appended_expression)
                .or_else(|| call.target.as_deref().and_then(ExpressionExt::appended_expression)),
            | Expr::Comprehension(comprehension) => comprehension.loop_step.appended_expression(),
            | _ => None,
        }
    }
    fn estimate(
        &self,
        scope: &BTreeMap<String, Estimate>,
        inventory: &Inventory,
        variables: &Map<String, Value>,
        limits: &SelectionLimits,
    ) -> Result<Estimate, SelectionError> {
        let estimated = match &self.expr {
            | Expr::Call(call) => call
                .target
                .as_deref()
                .map_or(Ok(None), |target| target.estimate(scope, inventory, variables, limits).map(Some))
                .and_then(|target| {
                    call.args
                        .iter()
                        .try_fold(Vec::new(), |mut values, argument| {
                            argument.estimate(scope, inventory, variables, limits).map(|value| {
                                values.push(value);
                                values
                            })
                        })
                        .and_then(|arguments| Function::from(call.func_name.as_str()).estimate(target, arguments))
                }),
            | Expr::Comprehension(comprehension) => {
                comprehension
                    .iter_range
                    .estimate(scope, inventory, variables, limits)
                    .and_then(|range| match &range.kind {
                        | ValueKind::List(element) => {
                            let iter = Estimate {
                                bytes: match range.length {
                                    | 0 => 0,
                                    | value => range.bytes.checked_div(value).unwrap_or_default(),
                                },
                                cost: 0,
                                kind: element.as_ref().clone(),
                                length: 0,
                                records: 1,
                            };
                            comprehension.accu_init.estimate(scope, inventory, variables, limits).and_then(|initial| {
                                let accumulator = Estimate { cost: 0, ..initial.clone() };
                                let mut nested = scope.clone();
                                nested.insert(comprehension.iter_var.clone(), iter);
                                nested.insert(comprehension.accu_var.clone(), accumulator);
                                comprehension.iter_var2.iter().for_each(|name| {
                                    let bytes = limits
                                        .variable_scalar
                                        .checked_bytes()
                                        .and_then(|value| usize::try_from(value).ok())
                                        .unwrap_or_default();
                                    nested.insert(name.clone(), Estimate::scalar(ValueKind::Unknown, bytes));
                                });
                                comprehension
                                    .loop_cond
                                    .estimate(&nested, inventory, variables, limits)
                                    .and_then(|condition| {
                                        comprehension.loop_step.estimate(&nested, inventory, variables, limits).and_then(|step| {
                                            comprehension.result.estimate(&nested, inventory, variables, limits).and_then(|result| {
                                                comprehension.loop_step.estimate_comprehension(
                                                    &comprehension.iter_var,
                                                    range,
                                                    initial,
                                                    condition,
                                                    step,
                                                    result,
                                                    &nested,
                                                    inventory,
                                                    variables,
                                                    limits,
                                                )
                                            })
                                        })
                                    })
                            })
                        }
                        | _ => Err(SelectionError::Check("CEL macros may iterate over lists only".to_string())),
                    })
            }
            | Expr::Ident(name) => scope
                .get(name)
                .cloned()
                .ok_or_else(|| SelectionError::Check(format!("cannot estimate variable {name}"))),
            | Expr::List(list) => list.elements.iter().try_fold(
                Estimate {
                    bytes: 0,
                    cost: 1,
                    kind: ValueKind::List(Box::new(ValueKind::Unknown)),
                    length: 0,
                    records: 0,
                },
                |mut list_estimate, element| {
                    element.estimate(scope, inventory, variables, limits).map(|element| {
                        list_estimate.bytes = list_estimate.bytes.saturating_add(element.bytes);
                        list_estimate.cost = list_estimate.cost.saturating_add(element.cost);
                        list_estimate.length = list_estimate.length.saturating_add(1);
                        list_estimate.records = list_estimate.records.saturating_add(element.records);
                        list_estimate.kind = match list_estimate.kind {
                            | ValueKind::List(current) if *current == ValueKind::Unknown || *current == element.kind => {
                                ValueKind::List(Box::new(element.kind))
                            }
                            | ValueKind::List(_) => ValueKind::List(Box::new(ValueKind::Unknown)),
                            | kind => kind,
                        };
                        list_estimate
                    })
                },
            ),
            | Expr::Literal(literal) => Ok(Estimate::literal(literal)),
            | Expr::Map(map) => map.entries.iter().try_fold(
                Estimate {
                    bytes: 0,
                    cost: 1,
                    kind: ValueKind::Map,
                    length: map.entries.len() as u64,
                    records: map.entries.len() as u64,
                },
                |mut map_estimate, entry| match &entry.expr {
                    | EntryExpr::MapEntry(entry) => entry.key.estimate(scope, inventory, variables, limits).and_then(|key| {
                        entry.value.estimate(scope, inventory, variables, limits).map(|value| {
                            map_estimate.bytes = map_estimate.bytes.saturating_add(key.bytes).saturating_add(value.bytes);
                            map_estimate.cost = map_estimate.cost.saturating_add(key.cost).saturating_add(value.cost);
                            map_estimate.records = map_estimate.records.saturating_add(value.records);
                            map_estimate
                        })
                    }),
                    | EntryExpr::StructField(_) => Err(SelectionError::Check("cannot estimate struct field".to_string())),
                },
            ),
            | Expr::Select(select) => select
                .operand
                .estimate(scope, inventory, variables, limits)
                .map(|operand| operand.select(self, &select.field, inventory, variables)),
            | Expr::Struct(_) | Expr::Unspecified => Err(SelectionError::Check("cannot estimate unsupported expression".to_string())),
        };
        estimated.and_then(|estimate| estimate.enforce(limits))
    }
    fn estimate_comprehension(
        &self,
        iter_var: &str,
        range: Estimate,
        initial: Estimate,
        condition: Estimate,
        step: Estimate,
        result: Estimate,
        scope: &BTreeMap<String, Estimate>,
        inventory: &Inventory,
        variables: &Map<String, Value>,
        limits: &SelectionLimits,
    ) -> Result<Estimate, SelectionError> {
        let iterations = range.length;
        let cost = range
            .cost
            .saturating_add(initial.cost)
            .saturating_add(iterations.saturating_mul(condition.cost.saturating_add(step.cost)))
            .saturating_add(result.cost);
        match initial.kind {
            | ValueKind::List(_) => self
                .appended_expression()
                .ok_or_else(|| SelectionError::Check("unsupported list-producing CEL macro form".to_string()))
                .and_then(|element_expression| {
                    element_expression
                        .estimate(scope, inventory, variables, limits)
                        .map(|element| (element_expression, element))
                })
                .map(|(element_expression, element)| {
                    let records = iterations.saturating_mul(element.records.max(1));
                    let direct_item = matches!(&element_expression.expr, Expr::Ident(name) if name == iter_var);
                    let selected_item_string = matches!(element.kind, ValueKind::ItemString);
                    let bytes = match direct_item || selected_item_string {
                        | true => range.bytes,
                        | false => iterations.saturating_mul(element.bytes),
                    };
                    Estimate {
                        bytes,
                        cost,
                        kind: ValueKind::List(Box::new(element.kind)),
                        length: iterations,
                        records,
                    }
                }),
            | _ => Ok(Estimate {
                bytes: result.bytes.max(1),
                cost,
                kind: result.kind,
                length: result.length,
                records: result.records,
            }),
        }
    }
    fn lookup<'a>(&self, variables: &'a Map<String, Value>) -> Option<&'a Value> {
        fn path(expression: &IdedExpr) -> Option<Vec<&str>> {
            match &expression.expr {
                | Expr::Ident(name) if name == "variables" => Some(Vec::new()),
                | Expr::Select(select) => path(&select.operand).map(|mut path| {
                    path.push(select.field.as_str());
                    path
                }),
                | _ => None,
            }
        }
        path(self).and_then(|path| {
            path.into_iter().fold(None, |current: Option<&Value>, field| match current {
                | None => variables.get(field),
                | Some(Value::Object(values)) => values.get(field),
                | Some(_) => None,
            })
        })
    }
}
impl Validate<ExpressionValidation<'_>> for IdedExpr {
    fn validate(&self, context: ExpressionValidation<'_>) -> Result<(), SelectionError> {
        let nodes = context.nodes.get().saturating_add(1);
        context.nodes.set(nodes);
        check_limit("ast_nodes", nodes, context.limits.ast_nodes)
            .and_then(|()| check_limit("ast_depth", context.depth, context.limits.ast_depth))
            .and_then(|()| match &self.expr {
                | Expr::Call(call) => Function::from(call.func_name.as_str()).validate(()).and_then(|()| {
                    let nested = ExpressionValidation {
                        depth: context.depth.saturating_add(1),
                        ..context
                    };
                    call.target
                        .as_deref()
                        .map_or(Ok(()), |target| target.validate(nested))
                        .and_then(|()| call.args.iter().try_for_each(|argument| argument.validate(nested)))
                }),
                | Expr::Comprehension(comprehension) => {
                    let nested = ExpressionValidation {
                        depth: context.depth.saturating_add(1),
                        ..context
                    };
                    comprehension
                        .iter_range
                        .validate(nested)
                        .and_then(|()| comprehension.accu_init.validate(nested))
                        .and_then(|()| {
                            let scope = context
                                .scope
                                .iter()
                                .cloned()
                                .chain([comprehension.iter_var.clone(), comprehension.accu_var.clone()])
                                .chain(comprehension.iter_var2.iter().cloned())
                                .collect::<BTreeSet<_>>();
                            let nested = ExpressionValidation { scope: &scope, ..nested };
                            comprehension
                                .loop_cond
                                .validate(nested)
                                .and_then(|()| comprehension.loop_step.validate(nested))
                                .and_then(|()| comprehension.result.validate(nested))
                        })
                }
                | Expr::Ident(name) if context.scope.contains(name) => Ok(()),
                | Expr::Ident(name) => Err(SelectionError::Check(format!("undeclared variable {name}"))),
                | Expr::List(list) => list.elements.iter().try_for_each(|element| {
                    element.validate(ExpressionValidation {
                        depth: context.depth.saturating_add(1),
                        ..context
                    })
                }),
                | Expr::Literal(LiteralValue::Bytes(_)) => Err(SelectionError::Check("bytes literals are not supported".to_string())),
                | Expr::Literal(_) => Ok(()),
                | Expr::Map(map) => map.entries.iter().try_for_each(|entry| match &entry.expr {
                    | EntryExpr::MapEntry(entry) => {
                        let nested = ExpressionValidation {
                            depth: context.depth.saturating_add(1),
                            ..context
                        };
                        entry.key.validate(nested).and_then(|()| entry.value.validate(nested))
                    }
                    | EntryExpr::StructField(_) => Err(SelectionError::Check("struct fields are not supported".to_string())),
                }),
                | Expr::Select(select) => select.operand.validate(ExpressionValidation {
                    depth: context.depth.saturating_add(1),
                    ..context
                }),
                | Expr::Struct(_) => Err(SelectionError::Check("struct literals are not supported".to_string())),
                | Expr::Unspecified => Err(SelectionError::Check("unspecified expression is not supported".to_string())),
            })
    }
}
impl Inventory {
    fn discover(options: &SelectionOptions, configuration_path: &Path, limits: &SelectionLimits) -> Result<Self, SelectionError> {
        let base = configuration_path
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        options
            .roots
            .iter()
            .enumerate()
            .try_fold(
                (DiscoveryAccumulator::default(), Vec::new()),
                |(state, mut roots), (index, configured)| {
                    resolve_root(state, base, configured, index, options.recursive, limits).map(|(state, root)| {
                        roots.push(root);
                        (state, roots)
                    })
                },
            )
            .map(|(state, roots)| Self {
                digest: inventory_digest(&state.items),
                items: state.items,
                options: ResolvedSelectionOptions {
                    recursive: options.recursive,
                    roots,
                },
                profile: state.profile,
                skipped_unsupported_entries: state.skipped_unsupported_entries,
            })
    }
    fn root_estimates(&self, variables: &Map<String, Value>) -> BTreeMap<String, Estimate> {
        let items = Estimate {
            bytes: self.profile.inventory_bytes as u64,
            cost: 1,
            kind: ValueKind::List(Box::new(ValueKind::Item)),
            length: self.items.len() as u64,
            records: self.items.len() as u64,
        };
        let options = Estimate {
            bytes: (self.options.roots.iter().map(String::len).sum::<usize>() as u64).saturating_add(1),
            cost: 1,
            kind: ValueKind::Options,
            length: 0,
            records: self.options.roots.len().saturating_add(1) as u64,
        };
        let variables_value = Value::Object(variables.clone());
        let mut variables_estimate = Estimate::json_value(&variables_value);
        variables_estimate.kind = ValueKind::Variables;
        BTreeMap::from([
            ("items".to_string(), items),
            ("options".to_string(), options),
            ("variables".to_string(), variables_estimate),
        ])
    }
}
impl SelectionOptions {
    pub(super) fn resolve(
        request: &SelectionRequest,
        variables: Option<&Map<String, Value>>,
        limits: &SelectionLimits,
    ) -> Result<Self, SelectionError> {
        let empty = Map::new();
        let variables = variables.unwrap_or(&empty);
        request
            .validate(limits)
            .and_then(|()| validate_variable_bindings(variables, limits))
            .and_then(|()| expand_roots(&request.options, variables, limits))
    }
}
impl Validate<&SelectionLimits> for SelectionRequest {
    fn validate(&self, limits: &SelectionLimits) -> Result<(), SelectionError> {
        match (self.options.roots.is_empty(), self.script.trim().is_empty()) {
            | (true, _) => Err(SelectionError::Request("options.roots must not be empty".to_string())),
            | (_, true) => Err(SelectionError::Request("script must not be empty".to_string())),
            | (false, false) => check_memory_limit("script", self.script.len() as u64, &limits.script)
                .and_then(|()| check_limit("roots", self.options.roots.len(), limits.roots)),
        }
    }
}
fn check_limit(resource: &'static str, actual: usize, limit: usize) -> Result<(), SelectionError> {
    check_limit_u64(resource, actual as u64, limit as u64)
}
fn check_limit_u64(resource: &'static str, actual: u64, limit: u64) -> Result<(), SelectionError> {
    match actual > limit {
        | true => Err(SelectionError::Limit { actual, limit, resource }),
        | false => Ok(()),
    }
}
fn check_memory_limit(resource: &'static str, actual: u64, limit: &Memory) -> Result<(), SelectionError> {
    check_limit_u64(resource, actual, limit.checked_bytes().unwrap_or_default())
}
fn digest_bytes(value: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value);
    format!("sha256:{:x}", hasher.finalize())
}
fn evaluate(
    program: &Program,
    inventory: &Inventory,
    variables: &Map<String, Value>,
    limits: &SelectionLimits,
) -> Result<Vec<Option<String>>, SelectionError> {
    cel::to_value(&inventory.items)
        .map_err(|error| SelectionError::Binding(error.to_string()))
        .and_then(|items| {
            cel::to_value(&inventory.options)
                .map_err(|error| SelectionError::Binding(error.to_string()))
                .map(|options| (items, options))
        })
        .and_then(|(items, options)| {
            cel::to_value(variables)
                .map_err(|error| SelectionError::Binding(error.to_string()))
                .map(|variables| (items, options, variables))
        })
        .and_then(|(items, options, variables)| {
            let mut context = Context::default();
            context.add_variable_from_value("items", items);
            context.add_variable_from_value("options", options);
            context.add_variable_from_value("variables", variables);
            program.execute(&context).map_err(|error| SelectionError::Evaluation(error.to_string()))
        })
        .and_then(|result| normalize_result(result, &inventory.items, limits))
}
fn expand_roots(options: &SelectionOptions, variables: &Map<String, Value>, limits: &SelectionLimits) -> Result<SelectionOptions, SelectionError> {
    let mut expansion = crate::interpolation::Expansion::new(
        variables,
        limits.variable_depth,
        limits.variable_nodes,
        limits.variable_scalar.checked_bytes().unwrap_or_default(),
    );
    options
        .roots
        .iter()
        .enumerate()
        .map(|(index, root)| {
            expansion
                .expand(root)
                .map_err(|error| match error {
                    | crate::interpolation::InterpolationError::Invalid(reason) => {
                        SelectionError::Request(format!("options.roots[{index}]: {reason}"))
                    }
                    | crate::interpolation::InterpolationError::Limit { resource, actual, limit } => SelectionError::Limit {
                        resource: match resource {
                            | "bytes" => "root_expansion_bytes",
                            | "depth" => "root_expansion_depth",
                            | _ => "root_expansion_substitutions",
                        },
                        actual,
                        limit,
                    },
                })
                .and_then(|expanded| {
                    if expanded.trim().is_empty() || expanded.contains('\0') || expanded.contains("{{") {
                        Err(SelectionError::Request(format!(
                            "options.roots[{index}]: expanded root must be nonempty and contain no NUL or unresolved placeholder"
                        )))
                    } else {
                        Ok(expanded)
                    }
                })
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|roots| SelectionOptions {
            recursive: options.recursive,
            roots,
        })
}
fn hash_field(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}
fn increment_limit(value: &mut usize, resource: &'static str, limit: usize) -> Result<(), SelectionError> {
    *value = value.saturating_add(1);
    check_limit(resource, *value, limit)
}
fn inspect_entry(
    mut state: DiscoveryAccumulator,
    root_index: usize,
    entry: DirEntry,
    relative_path: String,
    depth: usize,
    recursive: bool,
    limits: &SelectionLimits,
) -> Result<DiscoveryAccumulator, SelectionError> {
    increment_limit(&mut state.visited_entries, "visited_entries", limits.visited_entries)
        .and_then(|()| increment_limit(&mut state.metadata_operations, "metadata_operations", limits.metadata_operations))
        .and_then(|()| check_limit("traversal_depth", depth, limits.traversal_depth))
        .and_then(|()| {
            let path = entry.path();
            symlink_metadata(&path)
                .map_err(|source| SelectionError::Discovery { path: path.clone(), source })
                .and_then(|metadata| {
                    let file_type = metadata.file_type();
                    match (file_type.is_symlink(), file_type.is_file(), file_type.is_dir()) {
                        | (true, _, _) | (false, false, false) => {
                            state.skipped_unsupported_entries = state.skipped_unsupported_entries.saturating_add(1);
                            Ok(state)
                        }
                        | (false, is_file, is_directory) => path
                            .to_str()
                            .map(str::to_string)
                            .ok_or_else(|| SelectionError::Request(format!("candidate path is not valid UTF-8: {}", path.display())))
                            .and_then(|candidate_path| {
                                let name = entry.file_name().to_string_lossy().into_owned();
                                let extension = Path::new(&name)
                                    .extension()
                                    .and_then(|value| value.to_str())
                                    .map_or_else(String::new, |value| format!(".{value}"));
                                let item = SelectionItem {
                                    depth,
                                    extension,
                                    index: root_index,
                                    kind: match is_file {
                                        | true => "file".to_string(),
                                        | false => "directory".to_string(),
                                    },
                                    name,
                                    path: candidate_path,
                                    relative_path: relative_path.clone(),
                                    size: match is_file {
                                        | true => metadata.len(),
                                        | false => 0,
                                    },
                                };
                                state.add_candidate(item, limits).and_then(|state| match recursive && is_directory {
                                    | true => state.visit_directory(root_index, &path, &relative_path, depth, recursive, limits),
                                    | false => Ok(state),
                                })
                            }),
                    }
                })
        })
}
fn inventory_digest(items: &[SelectionItem]) -> String {
    let mut hasher = Sha256::new();
    hash_field(&mut hasher, INVENTORY_DIGEST_VERSION);
    items.iter().for_each(|item| {
        hash_field(&mut hasher, &(item.index as u64).to_be_bytes());
        hash_field(&mut hasher, item.path.as_bytes());
        hash_field(&mut hasher, item.relative_path.as_bytes());
        hash_field(&mut hasher, item.name.as_bytes());
        hash_field(&mut hasher, item.extension.as_bytes());
        hash_field(&mut hasher, item.kind.as_bytes());
        hash_field(&mut hasher, &item.size.to_be_bytes());
        hash_field(&mut hasher, &(item.depth as u64).to_be_bytes());
    });
    format!("sha256:{:x}", hasher.finalize())
}

fn normalize_result(result: CelValue, items: &[SelectionItem], limits: &SelectionLimits) -> Result<Vec<Option<String>>, SelectionError> {
    match result {
        | CelValue::List(values) => check_limit("result_records", values.len(), limits.intermediate_records).and_then(|()| {
            let candidates = items.iter().map(|item| item.path.as_str()).collect::<BTreeSet<_>>();
            values
                .iter()
                .try_fold(
                    (Vec::new(), BTreeSet::new(), 0usize),
                    |(mut selected, mut seen, bytes), value| match value {
                        | CelValue::String(value) if value.is_empty() => Err(SelectionError::Result("paths must not be empty".to_string())),
                        | CelValue::String(value) if value.contains('\0') => Err(SelectionError::Result("paths must not contain NUL".to_string())),
                        | CelValue::String(value) if !candidates.contains(value.as_str()) => Err(SelectionError::UnknownCandidate(value.to_string())),
                        | CelValue::String(value) => {
                            let next_bytes = bytes.saturating_add(value.len());
                            check_memory_limit("result_bytes", next_bytes as u64, &limits.intermediate).map(|()| {
                                if seen.insert(value.to_string()) {
                                    selected.push(value.to_string());
                                }
                                (selected, seen, next_bytes)
                            })
                        }
                        | value => Err(SelectionError::Result(format!(
                            "expected list<string>, found {} element",
                            value.type_of()
                        ))),
                    },
                )
                .map(|(selected, _, _)| match selected.is_empty() {
                    | true => vec![None],
                    | false => selected.into_iter().map(Some).collect(),
                })
        }),
        | value => Err(SelectionError::Result(format!("expected list<string>, found {}", value.type_of()))),
    }
}
fn read_entries(directory: &Path, visited_entries: usize, visited_limit: usize) -> Result<Vec<(String, DirEntry)>, SelectionError> {
    let remaining = visited_limit.saturating_sub(visited_entries);
    read_dir(directory)
        .map_err(|source| SelectionError::Discovery {
            path: directory.to_path_buf(),
            source,
        })
        .and_then(|entries| {
            entries
                .take(remaining.saturating_add(1))
                .map(|entry| {
                    entry
                        .map_err(|source| SelectionError::Discovery {
                            path: directory.to_path_buf(),
                            source,
                        })
                        .and_then(|entry| {
                            entry.file_name().into_string().map(|name| (name, entry)).map_err(|name| {
                                SelectionError::Request(format!(
                                    "candidate beneath {} is not valid UTF-8: {}",
                                    directory.display(),
                                    name.to_string_lossy()
                                ))
                            })
                        })
                })
                .collect::<Result<Vec<_>, _>>()
                .and_then(|mut entries| {
                    check_limit("visited_entries", visited_entries.saturating_add(entries.len()), visited_limit).map(|()| {
                        entries.sort_by(|left, right| left.0.cmp(&right.0));
                        entries
                    })
                })
        })
}
pub(super) fn resolve(
    request: &SelectionRequest,
    variables: Option<&Map<String, Value>>,
    configuration_path: &Path,
) -> Result<ResolvedInputs, SelectionError> {
    let limits = SelectionLimits::default();
    SelectionOptions::resolve(request, variables, &limits)
        .and_then(|options| Inventory::discover(&options, configuration_path, &limits))
        .and_then(|inventory| {
            let variables = variables.cloned().unwrap_or_default();
            Cel.compile(&request.script, &inventory, &variables, &limits)
                .and_then(|script| Cel.evaluate(&script, &inventory, &variables, &limits))
                .map(|values| {
                    let metadata = Cel.metadata();
                    ResolvedInputs {
                        source: InputSource::Selection {
                            candidate_count: inventory.items.len(),
                            engine: request.engine,
                            interpreter: metadata.interpreter.to_string(),
                            interpreter_version: metadata.interpreter_version.to_string(),
                            inventory_digest: inventory.digest,
                            language: metadata.language.to_string(),
                            language_version: metadata.language_version.to_string(),
                            limits: Box::new(limits),
                            options: inventory.options,
                            script_digest: digest_bytes(request.script.as_bytes()),
                            skipped_unsupported_entries: inventory.skipped_unsupported_entries,
                        },
                        values,
                    }
                })
        })
}
fn resolve_root(
    mut state: DiscoveryAccumulator,
    base: &Path,
    configured: &str,
    index: usize,
    recursive: bool,
    limits: &SelectionLimits,
) -> Result<(DiscoveryAccumulator, String), SelectionError> {
    match (
        configured.trim().is_empty(),
        configured.contains("://") || configured.starts_with("file:"),
    ) {
        | (true, _) => Err(SelectionError::Request(format!("options.roots[{index}] must not be empty"))),
        | (_, true) => Err(SelectionError::Request(format!("options.roots[{index}] must be a local directory path"))),
        | (false, false) => {
            let path = match Path::new(configured).is_absolute() {
                | true => PathBuf::from(configured),
                | false => base.join(configured),
            };
            increment_limit(&mut state.metadata_operations, "metadata_operations", limits.metadata_operations).and_then(|()| {
                symlink_metadata(&path)
                    .map_err(|source| SelectionError::Discovery { path: path.clone(), source })
                    .and_then(|metadata| match (metadata.file_type().is_symlink(), metadata.is_dir()) {
                        | (true, _) => Err(SelectionError::Request(format!("options.roots[{index}] must not be a symlink"))),
                        | (false, false) => Err(SelectionError::Request(format!("options.roots[{index}] is not a directory"))),
                        | (false, true) => canonicalize(&path)
                            .map_err(|source| SelectionError::Discovery { path: path.clone(), source })
                            .and_then(|root| {
                                root.to_str()
                                    .map(str::to_string)
                                    .ok_or_else(|| SelectionError::Request(format!("options.roots[{index}] is not valid UTF-8")))
                                    .and_then(|root_string| {
                                        let first_candidate = state.items.len();
                                        state.visit_directory(index, &root, "", 0, recursive, limits).map(|mut state| {
                                            if let Some(items) = state.items.get_mut(first_candidate..) {
                                                items.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
                                            }
                                            (state, root_string)
                                        })
                                    })
                            }),
                    })
            })
        }
    }
}
fn validate_json(value: &Value, depth: usize, counter: &mut VariableCounter, limits: &SelectionLimits) -> Result<(), SelectionError> {
    increment_limit(&mut counter.nodes, "variable_nodes", limits.variable_nodes)
        .and_then(|()| check_limit("variable_depth", depth, limits.variable_depth))
        .and_then(|()| match value {
            | Value::Array(values) => values
                .iter()
                .try_for_each(|value| validate_json(value, depth.saturating_add(1), counter, limits)),
            | Value::Object(values) => values.iter().try_for_each(|(key, value)| {
                check_memory_limit("variable_scalar", key.len() as u64, &limits.variable_scalar)
                    .and_then(|()| validate_json(value, depth.saturating_add(1), counter, limits))
            }),
            | Value::String(value) => {
                counter.bytes = counter.bytes.saturating_add(value.len());
                check_memory_limit("variable_scalar", value.len() as u64, &limits.variable_scalar)
            }
            | Value::Bool(_) | Value::Null | Value::Number(_) => Ok(()),
        })
}
fn validate_variable_bindings(variables: &Map<String, Value>, limits: &SelectionLimits) -> Result<(), SelectionError> {
    let mut counter = VariableCounter::default();
    validate_json(&Value::Object(variables.clone()), 0, &mut counter, limits)
}

#[cfg(test)]
mod tests;
