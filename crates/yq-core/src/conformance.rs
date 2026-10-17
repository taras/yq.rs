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
}

/// The fixed clock yq's test suite runs under (`operators_test.go`, `TestMain`).
pub const TEST_NOW: &str = "2021-05-19T01:02:03.000000004Z";

/// Runs one scenario and returns the result strings in yq's test format
/// (`D<doc>, P<path>, (<tag>)::<yaml>`), or the error message.
pub fn run_scenario(scenario: &Scenario) -> Result<Vec<String>, String> {
    let mut arena = Arena::new();
    let inputs = read_inputs(&mut arena, scenario).map_err(|e| e.0)?;
    let results =
        evaluate_expression(&mut arena, &scenario.expression, inputs, scenario).map_err(|e| e.0)?;
    let mut printed = Vec::new();
    for id in results {
        printed.push(result_to_string(&mut arena, id).map_err(|e| e.0)?);
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

fn evaluate_expression(
    _arena: &mut Arena,
    expression: &str,
    inputs: Vec<NodeId>,
    _scenario: &Scenario,
) -> Result<Vec<NodeId>, Error> {
    if expression.trim() == "." {
        return Ok(inputs);
    }
    Err(Error(format!(
        "expression parser not implemented yet: {expression}"
    )))
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
