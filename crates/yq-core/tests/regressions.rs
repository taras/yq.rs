//! Behaviour the upstream fixture does not cover, checked against the Go yq
//! binary at the pinned reference commit.

use yq_core::{evaluate, EvalOptions, Format};

fn eval(expression: &str, input: &str, options: EvalOptions) -> String {
    evaluate(expression, input, &options).expect("evaluation succeeds")
}

#[test]
fn line_comment_after_quoted_scalar_is_kept() {
    let options = EvalOptions::default();
    assert_eq!(
        eval(".", "a: \"x\"   # c\n", options.clone()),
        "a: \"x\" # c\n"
    );
    assert_eq!(eval(".", "a: 'x' # c\n", options.clone()), "a: 'x' # c\n");
    assert_eq!(
        eval(".", "a:\n  - b: \"x\" # c\n    d: 1\n", options.clone()),
        "a:\n  - b: \"x\" # c\n    d: 1\n"
    );
    assert_eq!(
        eval(".a.b = \"y\"", "a:\n  b: \"x\" # c\n  d: 1\n", options),
        "a:\n  b: \"y\" # c\n  d: 1\n"
    );
}

#[test]
fn json_output_keeps_string_quotes_by_default() {
    let json = EvalOptions {
        output_format: Format::Json,
        ..EvalOptions::default()
    };
    assert_eq!(eval(".a", "a: x\n", json.clone()), "\"x\"\n");
    assert_eq!(
        eval(
            ".a",
            "a: x\n",
            EvalOptions {
                unwrap_scalar: Some(true),
                ..json
            }
        ),
        "x\n"
    );
}

#[test]
fn yaml_output_unwraps_scalars_by_default() {
    assert_eq!(eval(".a", "a: \"x\" # c\n", EvalOptions::default()), "x\n");
    assert_eq!(
        eval(
            ".a",
            "a: \"x\" # c\n",
            EvalOptions {
                unwrap_scalar: Some(false),
                ..EvalOptions::default()
            }
        ),
        "\"x\" # c\n"
    );
}

#[test]
fn flow_collection_may_close_at_parent_column_after_quoted_scalar() {
    let options = EvalOptions::default();
    assert_eq!(
        eval(".", "a: {\"k\": \"v\"\n}\n", options.clone()),
        "a: {\"k\": \"v\"}\n"
    );
    assert_eq!(eval(".", "a: [\"x\"\n]\n", options.clone()), "a: [\"x\"]\n");
    assert_eq!(
        eval(".", "a: {\"k\": \"v\" # c\n}\n", options),
        "a: {\"k\": \"v\", # c\n}\n"
    );
}
