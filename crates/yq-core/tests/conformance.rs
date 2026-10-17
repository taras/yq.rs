//! Conformance ratchet against yq's own operator scenarios.
//!
//! The test passes as long as the number of passing scenarios does not drop
//! below `tests/conformance_baseline.txt`. Raise the baseline in the same
//! commit that makes more scenarios pass. It fails on any regression.
//!
//! Environment:
//!   YQ_CONFORMANCE_GROUP=<substring>  only run groups whose name contains it
//!   YQ_CONFORMANCE_VERBOSE=1          print expected/actual for each failure
//!
//! A full report of failures is written to `target/conformance-report.txt`.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use yq_core::conformance::{run_scenario, Scenario};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawScenario {
    group: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    document: String,
    #[serde(default)]
    document2: String,
    expression: String,
    #[serde(default)]
    expected: Option<Vec<String>>,
    #[serde(default)]
    expected_error: String,
    #[serde(default)]
    environment_variables: Option<BTreeMap<String, String>>,
    #[serde(default)]
    requires_format: String,
}

fn load_scenarios() -> Vec<RawScenario> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/scenarios.json");
    let data = std::fs::read_to_string(path).expect("read scenarios.json");
    serde_json::from_str(&data).expect("parse scenarios.json")
}

/// The example files `load` scenarios read, under the relative path the Go
/// tests use (`../../examples/<name>` from `pkg/yqlib`).
fn load_example_files() -> Vec<(String, String)> {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/examples");
    let mut files = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Ok(contents) = std::fs::read_to_string(entry.path()) {
                files.push((format!("../../examples/{name}"), contents));
            }
        }
    }
    files
}

fn read_baseline() -> usize {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/conformance_baseline.txt"
    );
    std::fs::read_to_string(path)
        .expect("read conformance_baseline.txt")
        .trim()
        .parse()
        .expect("baseline is an integer")
}

#[test]
fn conformance() {
    let group_filter = std::env::var("YQ_CONFORMANCE_GROUP").ok();
    let verbose = std::env::var("YQ_CONFORMANCE_VERBOSE").is_ok();
    let scenarios = load_scenarios();
    let example_files = load_example_files();

    let mut passed = 0usize;
    let mut skipped = 0usize;
    let mut failures: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    let mut per_group: BTreeMap<String, (usize, usize)> = BTreeMap::new();

    for raw in &scenarios {
        if let Some(filter) = &group_filter {
            if !raw.group.contains(filter.as_str()) {
                continue;
            }
        }
        // XML is out of scope for yq.rs; the json-only scenarios need the
        // alternate decoder which the runner does not expose.
        if !raw.requires_format.is_empty() {
            skipped += 1;
            continue;
        }
        let expected = raw.expected.clone().unwrap_or_default();
        let scenario = Scenario {
            group: raw.group.clone(),
            description: raw.description.clone(),
            document: raw.document.clone(),
            document2: raw.document2.clone(),
            expression: raw.expression.clone(),
            expected: expected.clone(),
            expected_error: raw.expected_error.clone(),
            environment_variables: raw
                .environment_variables
                .clone()
                .unwrap_or_default()
                .into_iter()
                .collect(),
            files: example_files.clone(),
        };
        let entry = per_group.entry(raw.group.clone()).or_insert((0, 0));
        entry.1 += 1;

        let outcome = run_scenario(&scenario);
        let ok = if raw.expected_error.is_empty() {
            matches!(&outcome, Ok(actual) if *actual == expected)
        } else {
            matches!(&outcome, Err(message) if message.contains(raw.expected_error.as_str()))
        };

        if ok {
            passed += 1;
            entry.0 += 1;
        } else {
            let mut detail = String::new();
            let _ = writeln!(detail, "[{}] {}", raw.group, raw.description);
            let _ = writeln!(detail, "  expression: {}", raw.expression);
            // The report file always carries the full detail; stdout only
            // does so when asked, to keep test output readable.
            let mut full = detail.clone();
            let _ = writeln!(full, "  document:   {:?}", raw.document);
            if !raw.document2.is_empty() {
                let _ = writeln!(full, "  document2:  {:?}", raw.document2);
            }
            if raw.expected_error.is_empty() {
                let _ = writeln!(full, "  expected:   {:?}", expected);
            } else {
                let _ = writeln!(full, "  expected error: {:?}", raw.expected_error);
            }
            let _ = writeln!(full, "  actual:     {:?}", outcome);
            if verbose {
                detail = full.clone();
            }
            failures
                .entry(raw.group.clone())
                .or_default()
                .push((detail, full));
        }
    }

    let total = passed + failures.values().map(Vec::len).sum::<usize>();
    let mut summary = String::new();
    let _ = writeln!(
        summary,
        "conformance: {passed}/{total} passed, {skipped} skipped"
    );
    for (group, (ok, count)) in &per_group {
        let _ = writeln!(summary, "  {group}: {ok}/{count}");
    }
    let mut report = summary.clone();
    let mut printed = summary;
    for details in failures.values() {
        for (detail, full) in details {
            printed.push_str(detail);
            report.push_str(full);
        }
    }
    let report_path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../target/conformance-report.txt"
    );
    if let Some(parent) = std::path::Path::new(report_path).parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(report_path, &report);
    println!("{printed}");

    if group_filter.is_some() {
        return;
    }
    let baseline = read_baseline();
    assert!(
        passed >= baseline,
        "conformance regressed: {passed} passed, baseline is {baseline}. See target/conformance-report.txt"
    );
    if passed > baseline {
        println!("conformance improved: {passed} > baseline {baseline}. Update tests/conformance_baseline.txt.");
    }
}
