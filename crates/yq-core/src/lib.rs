//! yq-core: the yq expression language, evaluated over a YAML/JSON node tree.
//!
//! The crate is a port of `pkg/yqlib` from <https://github.com/mikefarah/yq>.
//! `README.md` describes the architecture and current compatibility gaps.
//! Conformance is measured by `tests/conformance.rs` against the scenarios
//! extracted from yq's own test suite in `tests/fixtures/scenarios.json`.

extern crate alloc;

/// Internal support for the upstream conformance fixture, outside the stable API.
#[doc(hidden)]
pub mod conformance;
mod context;
mod eval;
mod json;
mod lexer;
mod node;
mod operation;
mod ops;
mod parser;
mod postfix;
mod printer;
mod util;
mod yaml;

/// Input and output serialisation formats the engine understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Yaml,
    Json,
}

/// Everything the host must supply, since the engine itself has no access to
/// the environment, the clock, or the filesystem (it runs inside wasm).
#[derive(Debug, Clone)]
#[non_exhaustive]
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
    /// File contents for `load`, `load_str`, `load_props` and
    /// `load_base64`, keyed by the exact path an expression will use.
    pub files: Vec<(String, String)>,
    /// Print scalar results bare, without quotes, styles or comments (yq's
    /// `-r`). `None` follows yq's default: on for YAML output, off for JSON.
    pub unwrap_scalar: Option<bool>,
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
            files: Vec::new(),
            unwrap_scalar: None,
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
///
/// Host capabilities are supplied explicitly through [`EvalOptions`]. No
/// environment, clock, filesystem or process access is performed by the engine.
///
/// ```
/// use yq_core::{evaluate, EvalOptions, Format};
///
/// let mut options = EvalOptions::default();
/// options.output_format = Format::Json;
/// assert_eq!(evaluate(".name", "name: yq\n", &options)?, "\"yq\"\n");
/// # Ok::<(), yq_core::Error>(())
/// ```
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
    let parsed = parser::parse_expression(expression)?;
    let mut evaluator = eval::Evaluator::new(arena);
    evaluator.env = options.env.clone();
    evaluator.now = options.now.clone();
    evaluator.files = options.files.clone();
    let results =
        evaluator.get_matching_nodes_opt(context::Context::new(inputs), parsed.as_ref())?;
    let mut printer = printer::Printer::new(printer::PrinterPrefs {
        format: options.output_format,
        indent: options.indent,
        unwrap_scalar: options
            .unwrap_scalar
            .unwrap_or(options.output_format == Format::Yaml),
        print_doc_separators: true,
    });
    let nodes = results.nodes().to_vec();
    printer.print_results(&mut evaluator.arena, &nodes)?;
    Ok(printer.into_output())
}
