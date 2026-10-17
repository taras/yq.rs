//! yq-core: the yq expression language, evaluated over a YAML/JSON node tree.
//!
//! The crate is a port of `pkg/yqlib` from <https://github.com/mikefarah/yq>.
//! `docs/design.md` describes the architecture; `PLAN.md` the build order.
//! Conformance is measured by `tests/conformance.rs` against the scenarios
//! extracted from yq's own test suite in `tests/fixtures/scenarios.json`.

extern crate alloc;

pub mod conformance;
pub mod json;
pub mod node;
pub mod printer;
pub mod util;
pub mod yaml;

/// Input and output serialisation formats the engine understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Yaml,
    Json,
}

/// Everything the host must supply, since the engine itself has no access to
/// the environment, the clock, or the filesystem (it runs inside wasm).
#[derive(Debug, Clone)]
pub struct EvalOptions {
    pub input_format: Format,
    pub output_format: Format,
    pub indent: usize,
    /// Evaluate the expression against a single null document instead of input.
    pub null_input: bool,
    /// Backing store for `env(NAME)`, `strenv(NAME)` and `envsubst`.
    pub env: Vec<(String, String)>,
    /// RFC 3339 timestamp returned by `now`. `None` makes `now` an error.
    pub now: Option<String>,
}

impl Default for EvalOptions {
    fn default() -> Self {
        EvalOptions {
            input_format: Format::Yaml,
            output_format: Format::Yaml,
            indent: 2,
            null_input: false,
            env: Vec::new(),
            now: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for Error {}

/// Evaluates `expression` against `input` and returns the printed results, the
/// way `yq eval` prints them to stdout.
pub fn evaluate(expression: &str, input: &str, options: &EvalOptions) -> Result<String, Error> {
    let mut arena = node::Arena::new();
    let inputs = if options.null_input {
        vec![arena.push(node::Node::scalar("!!null", ""))]
    } else {
        match options.input_format {
            Format::Yaml => yaml::decode::decode_documents(&mut arena, input, "", 0)?,
            Format::Json => json::decode::decode_documents(&mut arena, input)?,
        }
    };
    let _ = expression;
    let mut printer = printer::Printer::new(printer::PrinterPrefs {
        format: options.output_format,
        indent: options.indent,
        unwrap_scalar: true,
        print_doc_separators: true,
    });
    for id in inputs {
        printer.print_node(&mut arena, id)?;
    }
    Ok(printer.into_output())
}
