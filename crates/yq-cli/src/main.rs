//! `yq-rs [flags] <expression> [file]`
//!
//! Deliberately tiny: it compares yq-core with the reference Go yq on the
//! same input locally. It is not the product.
//! Flags mirror the yq ones they copy: -p/--input-format, -o/--output-format,
//! -I/--indent, -n/--null-input, -r/--unwrapScalar.

use std::io::Read;
use yq_core::{EvalOptions, Format};

fn parse_format(value: &str) -> Format {
    match value {
        "json" | "j" => Format::Json,
        _ => Format::Yaml,
    }
}

fn main() {
    let mut options = EvalOptions {
        env: std::env::vars().collect(),
        now: None,
        ..EvalOptions::default()
    };
    let mut positional = Vec::new();
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-p" | "--input-format" => {
                options.input_format = parse_format(&args.next().unwrap_or_default())
            }
            "-o" | "--output-format" => {
                options.output_format = parse_format(&args.next().unwrap_or_default())
            }
            "-I" | "--indent" => {
                options.indent = args.next().and_then(|v| v.parse().ok()).unwrap_or(2)
            }
            "-n" | "--null-input" => options.null_input = true,
            "-r" | "--unwrapScalar" | "--unwrapScalar=true" => options.unwrap_scalar = Some(true),
            "-r=false" | "--unwrapScalar=false" => options.unwrap_scalar = Some(false),
            _ => positional.push(arg),
        }
    }
    if positional.is_empty() {
        eprintln!("usage: yq-rs [-p fmt] [-o fmt] [-I n] [-n] [-r[=false]] <expression> [file]");
        std::process::exit(2);
    }
    let expression = positional.remove(0);
    let mut input = String::new();
    if options.null_input {
        // nothing to read
    } else if let Some(path) = positional.first().filter(|p| p.as_str() != "-") {
        input = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) => {
                eprintln!("Error: open {path}: {error}");
                std::process::exit(1);
            }
        };
    } else if std::io::stdin().read_to_string(&mut input).is_err() {
        eprintln!("Error: could not read stdin");
        std::process::exit(1);
    }
    match yq_core::evaluate(&expression, &input, &options) {
        Ok(output) => print!("{output}"),
        Err(error) => {
            eprintln!("Error: {error}");
            std::process::exit(1);
        }
    }
}
