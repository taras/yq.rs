use napi_derive::napi;
use std::collections::HashMap;
use yq_core::{EvalOptions, Format};

#[napi(object)]
#[derive(Default)]
pub struct Options {
    pub input_format: Option<String>,
    pub output_format: Option<String>,
    pub indent: Option<f64>,
    pub null_input: Option<bool>,
    pub env: Option<HashMap<String, String>>,
    pub files: Option<HashMap<String, String>>,
    pub now: Option<String>,
    pub unwrap_scalar: Option<bool>,
    pub print_doc_separators: Option<bool>,
}

fn format(value: Option<String>, default: Format) -> Result<Format, String> {
    match value.as_deref() {
        None => Ok(default),
        Some("yaml" | "y") => Ok(Format::Yaml),
        Some("json" | "j") => Ok(Format::Json),
        Some(other) => Err(format!("unknown format: {other}")),
    }
}

impl Options {
    fn into_core(self) -> Result<EvalOptions, String> {
        let mut options = EvalOptions::default();
        options.input_format = format(self.input_format, options.input_format)?;
        options.output_format = format(self.output_format, options.output_format)?;
        if let Some(indent) = self.indent {
            if !indent.is_finite()
                || indent < 0.0
                || indent.fract() != 0.0
                || indent > u32::MAX as f64
            {
                return Err(format!("bad indent: {indent}"));
            }
            options.indent = indent as usize;
        }
        options.null_input = self.null_input.unwrap_or(options.null_input);
        options.env = self.env.unwrap_or_default().into_iter().collect();
        options.files = self.files.unwrap_or_default().into_iter().collect();
        options.now = self.now;
        options.unwrap_scalar = self.unwrap_scalar;
        options.print_doc_separators = self
            .print_doc_separators
            .unwrap_or(options.print_doc_separators);
        Ok(options)
    }
}

#[napi(object)]
pub struct Outcome {
    pub output: String,
    pub matched: bool,
    pub error: Option<String>,
}

impl Outcome {
    fn success(value: yq_core::Evaluation) -> Self {
        Self {
            output: value.output,
            matched: value.matched,
            error: None,
        }
    }
    fn failure(error: String, output: String) -> Self {
        Self {
            output,
            matched: false,
            error: Some(error),
        }
    }
}

#[napi]
pub fn evaluate(expression: String, input: String, options: Option<Options>) -> Outcome {
    evaluate_with_status(expression, input, options)
}

#[napi]
pub fn evaluate_with_status(
    expression: String,
    input: String,
    options: Option<Options>,
) -> Outcome {
    match options.unwrap_or_default().into_core() {
        Ok(options) => match yq_core::evaluate_with_status(&expression, &input, &options) {
            Ok(value) => Outcome::success(value),
            Err(error) => Outcome::failure(error.0, String::new()),
        },
        Err(error) => Outcome::failure(error, String::new()),
    }
}

#[napi]
pub fn evaluate_sequence(
    expression: String,
    inputs: Vec<String>,
    options: Option<Options>,
) -> Outcome {
    match options.unwrap_or_default().into_core() {
        Ok(options) => {
            let inputs: Vec<&str> = inputs.iter().map(String::as_str).collect();
            match yq_core::evaluate_sequence_with_status(&expression, &inputs, &options) {
                Ok(value) => Outcome::success(value),
                Err(error) => Outcome::failure(error.error.0, error.output),
            }
        }
        Err(error) => Outcome::failure(error, String::new()),
    }
}
