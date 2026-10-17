use yq_core::{evaluate, evaluate_with_status, EvalOptions, Format};

#[test]
fn exit_status_uses_nodes_instead_of_rendered_text() {
    let mut options = EvalOptions::default();
    options.null_input = true;
    for expression in ["false", "null", "select(false)"] {
        assert!(
            !evaluate_with_status(expression, "", &options)
                .unwrap()
                .matched
        );
    }
    for expression in [
        "\"false\"",
        "\"null\"",
        "\"\"",
        "0",
        "[]",
        "{}",
        "false, true",
        "true, false",
    ] {
        assert!(
            evaluate_with_status(expression, "", &options)
                .unwrap()
                .matched
        );
    }
    let boolean = evaluate_with_status("false", "", &options).unwrap();
    let string = evaluate_with_status("\"false\"", "", &options).unwrap();
    assert_eq!(boolean.output, string.output);
    assert_eq!(evaluate("false", "", &options).unwrap(), boolean.output);
}

#[test]
fn separators_can_be_disabled_without_rewriting_scalar_contents() {
    let mut options = EvalOptions::default();
    let input = "---\na: |\n  ---\n---\na: next\n";
    assert_eq!(
        evaluate(".a", input, &options).unwrap(),
        "---\n\n---\nnext\n"
    );
    options.print_doc_separators = false;
    assert_eq!(evaluate(".a", input, &options).unwrap(), "---\n\nnext\n");
    options.output_format = Format::Json;
    assert_eq!(
        evaluate(".a", input, &options).unwrap(),
        "\"---\\n\"\n\"next\"\n"
    );
}

#[test]
fn sequential_inputs_share_a_printer_but_keep_evaluation_independent() {
    use yq_core::evaluate_sequence_with_status;
    let options = EvalOptions::default();
    let inputs = ["name: Ada\n", "---\nname: Grace\n"];
    let result = evaluate_sequence_with_status(".name", &inputs, &options).unwrap();
    assert_eq!(result.output, "Ada\n---\nGrace\n");
    assert!(result.matched);
    assert_eq!(
        evaluate_sequence_with_status("length", &inputs, &options)
            .unwrap()
            .output,
        "1\n1\n"
    );
    assert_eq!(
        evaluate_sequence_with_status("[.name]", &inputs, &options)
            .unwrap()
            .output,
        "- Ada\n- Grace\n"
    );
    assert_eq!(
        evaluate_sequence_with_status("fileIndex", &inputs, &options)
            .unwrap()
            .output,
        "0\n1\n"
    );
    assert_eq!(
        evaluate_sequence_with_status("select(fileIndex == 1)", &inputs, &options)
            .unwrap()
            .output,
        "---\nname: Grace\n"
    );
}

#[test]
fn sequential_errors_retain_earlier_output() {
    use yq_core::evaluate_sequence_with_status;
    let options = EvalOptions::default();
    let failure =
        evaluate_sequence_with_status(".name", &["name: Ada\n", "[invalid"], &options).unwrap_err();
    assert_eq!(failure.output, "Ada\n");
    assert!(!failure.error.0.is_empty());
    let failure = evaluate_sequence_with_status(
        ".value | from_json",
        &["value: 'true'\n", "value: '['\n"],
        &options,
    )
    .unwrap_err();
    assert_eq!(failure.output, "true\n");
}
