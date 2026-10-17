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
    let mut json = EvalOptions::default();
    json.output_format = Format::Json;
    assert_eq!(eval(".a", "a: x\n", json.clone()), "\"x\"\n");
    json.unwrap_scalar = Some(true);
    assert_eq!(eval(".a", "a: x\n", json), "x\n");
}

#[test]
fn yaml_output_unwraps_scalars_by_default() {
    assert_eq!(eval(".a", "a: \"x\" # c\n", EvalOptions::default()), "x\n");
    let mut options = EvalOptions::default();
    options.unwrap_scalar = Some(false);
    assert_eq!(eval(".a", "a: \"x\" # c\n", options), "\"x\" # c\n");
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

#[test]
fn leading_yaml_bom_is_not_part_of_the_first_key() {
    for input in [
        "\u{feff}name: Ada\n",
        "\u{feff}name: Ada\r\n",
        "\u{feff}{name: Ada}\n",
        "\u{feff}# heading\nname: Ada\n",
        "\u{feff}---\nname: Ada\n",
        "\u{feff}%YAML 1.1\n---\nname: Ada\n",
        "# heading\n\u{feff}name: Ada\n",
    ] {
        assert_eq!(eval(".name", input, EvalOptions::default()), "Ada\n");
    }
    let input = "\u{feff}name: Ada\n";
    assert_eq!(eval(".", input, EvalOptions::default()), "name: Ada\n");
    assert_eq!(eval("line", input, EvalOptions::default()), "1\n");
    assert_eq!(eval("column", input, EvalOptions::default()), "1\n");
}

#[test]
fn yaml_bom_is_consumed_after_header_preprocessing() {
    for (input, expected) in [
        ("\u{feff}---\nname: Ada\n", "name: Ada\n"),
        ("\u{feff}# heading\nname: Ada\n", "# heading\nname: Ada\n"),
        ("\n\u{feff}name: Ada\n", "\nname: Ada\n"),
    ] {
        assert_eq!(eval(".", input, EvalOptions::default()), expected);
    }
}

#[test]
fn yaml_bom_is_handled_in_loaded_and_decoded_strings() {
    let mut options = EvalOptions::default();
    options.null_input = true;
    options.files = vec![("bom.yml".into(), "\u{feff}name: Ada\n".into())];
    assert_eq!(
        eval(r#"load("bom.yml").name"#, "", options.clone()),
        "Ada\n"
    );
    assert_eq!(
        eval("\"\u{feff}name: Ada\\n\" | from_yaml | .name", "", options),
        "Ada\n"
    );
}

#[test]
fn in_stream_and_scalar_bom_characters_are_preserved() {
    for input in [
        "name: \u{feff}Ada\n",
        "name: \"\u{feff}Ada\"\n",
        "\u{feff}name: \"\u{feff}Ada\"\n",
    ] {
        assert_eq!(
            eval(".name", input, EvalOptions::default()),
            "\u{feff}Ada\n"
        );
    }
    let input = "first: 1\n\u{feff}name: Ada\n";
    assert_eq!(eval(".name", input, EvalOptions::default()), "null\n");
    assert_eq!(
        eval(".[\"\u{feff}name\"]", input, EvalOptions::default()),
        "Ada\n"
    );
}

#[test]
fn json_rejects_a_leading_bom_but_preserves_it_in_strings() {
    let mut options = EvalOptions::default();
    options.input_format = Format::Json;
    assert!(evaluate(".name", "\u{feff}{\"name\":\"Ada\"}", &options).is_err());
    assert_eq!(
        eval(".name", "{\"name\":\"\u{feff}Ada\"}", options),
        "\u{feff}Ada\n"
    );
}
