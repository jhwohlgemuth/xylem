use super::*;
use crate::completion::{Completion, CompletionPlan};
use acorn_core::validation::{digest, ValidationError};
use acorn_schema::validation::{report_of, rules, Validate, ValidationReport};
use core::mem;
use glob::Pattern;

impl Validate for Completion {
    fn validate(&self) -> Result<(), ValidationReport> {
        let mut report = ValidationReport::new();
        self.marker.as_ref().into_iter().for_each(|marker| {
            if let Err(error) = rules::nonempty(marker) {
                report.add("marker", error.with_message("marker must not be empty"));
            } else if marker.contains(['*', '?', '[', ']']) {
                report.add(
                    "marker",
                    validation_error("exact_path", "marker must be an exact path without glob syntax"),
                );
            }
        });
        if self.outputs.is_empty() {
            report.add("outputs", validation_error("nonempty", "outputs must not be empty"));
        }
        self.outputs.iter().enumerate().for_each(|(index, output)| {
            if let Err(error) = rules::nonempty(output) {
                report.add(format!("outputs[{index}]"), error.with_message("output pattern must not be empty"));
            } else if let Err(error) = Pattern::new(output) {
                report.add(
                    format!("outputs[{index}]"),
                    validation_error("glob", format!("invalid glob pattern: {error}")),
                );
            }
        });
        report.finish()
    }
}
impl Config {
    /// Validate completion declarations after concrete inputs and variables are known.
    pub fn validate_for_inputs(&self, inputs: &[Option<String>]) -> Result<(), ValidationReport> {
        let mut report = report_of(Validate::validate(self));
        let mut markers = BTreeSet::new();
        inputs.iter().enumerate().for_each(|(input_index, input)| {
            self.modules.iter().enumerate().for_each(|(module_index, module)| {
                let path = format!("inputs[{input_index}].modules[{module_index}].completion");
                let variables = self.clone().get_environment_variables(Some(module.clone()), input.clone());
                match CompletionPlan::resolve(module, &variables, self.output.as_deref()) {
                    | Ok(Some(completion)) if !markers.insert(completion.marker.clone()) => report.add(
                        format!("{path}.marker"),
                        validation_error("unique", format!("resolved marker collides at {}", completion.marker.display())),
                    ),
                    | Ok(_) => (),
                    | Err(error) => report.add(path, validation_error("completion", error.to_string())),
                }
            });
        });
        report.finish()
    }
}
impl Validate for Config {
    fn validate(&self) -> Result<(), ValidationReport> {
        let mut report = ValidationReport::new();
        if matches!(self.input, Some(InputAttribute::ShellScript(_))) {
            report.add("input", validation_error("unsupported", "shell input is not supported"));
        }
        if self.check.is_some() {
            report.add(
                "check",
                validation_error("unsupported", "workflow checks are not supported during execution"),
            );
        }
        if self.resources.is_some() {
            report.add(
                "resources",
                validation_error("unsupported", "resource declarations are not supported during execution"),
            );
        }
        if let Some(output) = &self.output {
            if let Err(error) = rules::nonempty(output) {
                report.add("output", error.with_message("workflow output must not be empty"));
            }
        }
        self.requirements.iter().enumerate().for_each(|(index, requirement)| {
            if let Err(error) = rules::nonempty(requirement) {
                report.add(format!("requirements[{index}]"), error.with_message("requirement name must not be empty"));
            }
        });
        if let Some(options) = &self.options {
            if options.root.is_some() {
                report.add(
                    "options.root",
                    validation_error("managed", "run root is managed by Xylem and cannot be configured"),
                );
            }
            if let Err(error) = RemotePolicy::from_hosts(options.allowed.as_deref()) {
                report.add("options.allowed", validation_error("hosts", error.to_string()));
            }
        }
        let skip_checksum = self.options.as_ref().and_then(|options| options.skip_verify_checksum).unwrap_or_default();
        self.modules.iter().enumerate().fold(BTreeSet::new(), |mut names, (index, module)| {
            let path = format!("modules[{index}]");
            if !module.name.trim().is_empty() && !names.insert(module.name.as_str()) {
                report.add(
                    format!("{path}.name"),
                    validation_error("unique", format!("duplicate module name {}", module.name)),
                );
            }
            report = mem::take(&mut report).merge(&path, report_of(module.validate()));
            if let ModuleType::Binary { checksum, uri } = &module.module_type {
                let is_remote = matches!(ModuleUri::from(uri), ModuleUri::Url(_));
                let missing_checksum = checksum.as_ref().is_none_or(|value| value.trim().is_empty());
                let missing_remote_checksum = is_remote && missing_checksum && !skip_checksum;
                if missing_remote_checksum {
                    report.add(
                        format!("{path}.checksum"),
                        validation_error(
                            "required",
                            "remote binary requires a checksum unless checksum verification is explicitly disabled",
                        ),
                    );
                }
            }
            if module.completion.is_some() {
                let has_output_directory = module
                    .variables
                    .as_ref()
                    .and_then(|variables| variables.get("OUTPUT_DIRECTORY"))
                    .or_else(|| self.variables.as_ref().and_then(|variables| variables.get("OUTPUT_DIRECTORY")))
                    .is_some();
                if self.output.is_none() && !has_output_directory {
                    report.add(
                        format!("{path}.completion"),
                        validation_error("required", "workflow output or OUTPUT_DIRECTORY is required"),
                    );
                }
            }
            names
        });
        validate_variables(self.variables.as_ref(), "variables", &mut report);
        report.finish()
    }
}
impl Validate for ContainerEnvironment {
    fn validate(&self) -> Result<(), ValidationReport> {
        let mut report = ValidationReport::new();
        if let Err(error) = rules::nonempty(&self.name) {
            report.add("name", error.with_message("container name must not be empty"));
        }
        if matches!(self.runtime, ContainerRuntime::Unknown) {
            report.add("runtime", validation_error("supported", "container runtime is not supported"));
        }
        if self.context.is_some() {
            report.add(
                "context",
                validation_error("unsupported", "container contexts are not supported during execution"),
            );
        }
        if self.image.is_some() {
            report.add(
                "image",
                validation_error("unsupported", "container image acquisition is not supported during execution"),
            );
        }
        report.finish()
    }
}
impl Validate for Module {
    fn validate(&self) -> Result<(), ValidationReport> {
        let mut report = ValidationReport::new();
        if let Err(error) = rules::nonempty(&self.name) {
            report.add("name", error.with_message("module name must not be empty"));
        }
        match &self.module_type {
            | ModuleType::Binary { checksum, uri } => {
                if let Err(error) = rules::url(uri) {
                    report.add("uri", error.with_message("URI must use file, HTTP, HTTPS, or SSH"));
                }
                let unsupported_remote = matches!(ModuleUri::from(uri), ModuleUri::Url(value) if !matches!(Scheme::from(value.scheme().as_str()), Scheme::HTTP | Scheme::HTTPS));
                if unsupported_remote {
                    report.add("uri", validation_error("scheme", "remote binary URI must use HTTP or HTTPS"));
                }
                if let Some(value) = checksum {
                    if let Err(error) = digest(value) {
                        report.add("checksum", error);
                    }
                }
            }
            | ModuleType::Configuration { uri } => {
                if let Err(error) = rules::url(uri) {
                    report.add("uri", error.with_message("URI must use file, HTTP, HTTPS, or SSH"));
                }
                report.add("type", validation_error("executable", "configuration modules are not executable"));
            }
            | ModuleType::Script { programming_language, uri } => {
                if let Err(error) = rules::url(uri) {
                    report.add("uri", error.with_message("URI must use file, HTTP, HTTPS, or SSH"));
                }
                if programming_language.as_ref().is_some_and(|language| !language.is_supported()) {
                    report.add("programmingLanguage", validation_error("supported", "script language is not supported"));
                }
            }
        }
        if self.before_script.is_some() {
            report.add("before_script", validation_error("unsupported", "lifecycle scripts are not supported"));
        }
        if self.after_script.is_some() {
            report.add("after_script", validation_error("unsupported", "lifecycle scripts are not supported"));
        }
        if let Some(template) = &self.template {
            report = report.merge("template", report_of(template.validate()));
        }
        if let Some(completion) = &self.completion {
            report = report.merge("completion", report_of(completion.validate()));
        }
        validate_variables(self.variables.as_ref(), "variables", &mut report);
        report.finish()
    }
}
impl Validate for TemplateAttribute {
    fn validate(&self) -> Result<(), ValidationReport> {
        let mut report = ValidationReport::new();
        match self {
            | Self::Path(value) => {
                if let Err(error) = rules::nonempty(value) {
                    report.add("", error.with_message("template path must not be empty"));
                }
            }
            | Self::Template(template) => {
                if let Err(error) = rules::nonempty(&template.command) {
                    report.add("command", error.with_message("template command must not be empty"));
                }
                if let Some(container) = &template.container {
                    report = report.merge("container", report_of(container.validate()));
                }
                if let Some(environment) = &template.virtual_environment {
                    if let Err(error) = rules::nonempty(&environment.name) {
                        report.add("environment.name", error.with_message("environment name must not be empty"));
                    }
                    if matches!(
                        environment.manager,
                        VirtualEnvironmentManager::Pyenv | VirtualEnvironmentManager::Unknown | VirtualEnvironmentManager::Virtualenv
                    ) {
                        report.add(
                            "environment.manager",
                            validation_error("supported", "environment manager is not supported during execution"),
                        );
                    }
                }
            }
        }
        report.finish()
    }
}
fn validate_variables(variables: Option<&Map<String, Value>>, path: &str, report: &mut ValidationReport) {
    variables.into_iter().flat_map(|values| values.iter()).for_each(|(name, value)| {
        if matches!(name.as_str(), "INPUT" | "MODULE_LIST" | "PARENT_DIRECTORY") {
            report.add(
                format!("{path}.{name}"),
                validation_error("reserved", "reserved variable cannot be configured"),
            );
        }
        if !value.is_string() && !value.is_boolean() && !value.is_number() {
            report.add(
                format!("{path}.{name}"),
                validation_error("type", "value must be a string, boolean, or number"),
            );
        }
    });
}
fn validation_error(code: &str, message: impl Into<String>) -> ValidationError {
    ValidationError::new(code).with_message(message)
}
