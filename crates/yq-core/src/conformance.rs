//! Test-only entry point that mirrors `testScenario` in yq's `operators_test.go`.
//!
//! It lives in the library (not under `tests/`) because it needs access to the
//! node tree to print results in yq's test format, which the public API hides.
//! The format of each result string is documented in
//! `tests/fixtures/README.md`; it is a contract with the fixture, not a
//! user-facing format.

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
    let _ = scenario;
    Err("yq-core: conformance runner is not implemented yet (see PLAN.md milestone 1)".to_string())
}
