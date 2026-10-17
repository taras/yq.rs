//! Test-only entry point that mirrors `testScenario` in yq's `operators_test.go`.
//!
//! It lives in the library (not under `tests/`) because it needs access to the
//! node tree to print results in yq's test format, which the public API hides.
//! The format of each result string is documented in
//! `tests/fixtures/README.md`; it is a contract with the fixture, not a
//! user-facing format.

use crate::node::{format_path, Arena, Kind, Node, NodeId};
use crate::printer::{Printer, PrinterPrefs};
use crate::yaml;
use crate::{Error, Format};

/// One scenario from `tests/fixtures/scenarios.json`.
#[derive(Debug, Clone, Default)]
pub struct Scenario {
    pub group: String,
    pub description: String,
    pub document: String,
    pub document2: String,
    pub expression: String,
    pub expected: Vec<String>,
    pub expected_error: String,
    pub environment_variables: Vec<(String, String)>,
    /// Files the `load` scenarios read, keyed by the path they use.
    pub files: Vec<(String, String)>,
}

/// The fixed clock yq's test suite runs under (`operators_test.go`, `TestMain`).
pub const TEST_NOW: &str = "2021-05-19T01:02:03.000000004Z";

/// Runs one scenario and returns the result strings in yq's test format
/// (`D<doc>, P<path>, (<tag>)::<yaml>`), or the error message.
pub fn run_scenario(scenario: &Scenario) -> Result<Vec<String>, String> {
    let parsed = crate::parser::parse_expression(&scenario.expression).map_err(|e| e.0)?;
    let mut arena = Arena::new();
    let inputs = read_inputs(&mut arena, scenario).map_err(|e| e.0)?;
    let mut evaluator = crate::eval::Evaluator::new(arena);
    evaluator.env = scenario.environment_variables.clone();
    evaluator.now = Some(TEST_NOW.to_string());
    evaluator.files = scenario.files.clone();
    // The Go suite runs the `fixed*` groups with the merge-anchor fix on.
    evaluator.fix_merge_anchor_to_spec = scenario.group.starts_with("fixed");
    evaluator.disable_env_ops = scenario.group == "envOperatorSecurityDisabledScenarios";
    evaluator.disable_file_ops = scenario.group == "loadOperatorSecurityDisabledScenarios";
    let results = evaluator
        .get_matching_nodes_opt(crate::context::Context::new(inputs), parsed.as_ref())
        .map_err(|e| e.0)?;
    let mut printed = Vec::new();
    for id in results.nodes().to_vec() {
        printed.push(result_to_string(&mut evaluator.arena, id).map_err(|e| e.0)?);
    }
    Ok(printed)
}

fn read_inputs(arena: &mut Arena, scenario: &Scenario) -> Result<Vec<NodeId>, Error> {
    if scenario.document.is_empty() {
        return Ok(vec![arena.push(Node::scalar("!!null", ""))]);
    }
    let mut inputs = yaml::decode::decode_documents(arena, &scenario.document, "sample.yml", 0)?;
    if !scenario.document2.is_empty() {
        let more = yaml::decode::decode_documents(arena, &scenario.document2, "another.yml", 1)?;
        inputs.extend(more);
    }
    Ok(inputs)
}

/// `resultToString`.
pub fn result_to_string(arena: &mut Arena, id: NodeId) -> Result<String, Error> {
    let mut printer = Printer::new(PrinterPrefs {
        format: Format::Yaml,
        indent: 4,
        unwrap_scalar: true,
        print_doc_separators: true,
    });
    printer.print_node(arena, id)?;
    let node = arena.get(id);
    let tag = if node.kind == Kind::Alias {
        "alias".to_string()
    } else {
        node.tag.clone()
    };
    Ok(format!(
        "D{}, P{}, ({})::{}",
        arena.get_document(id),
        format_path(&arena.get_path(id)),
        tag,
        printer.into_output()
    ))
}
