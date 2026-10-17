use std::io::{Read, Write};
use yq_core::{EvalOptions, Format};

const REFERENCE: &str = "v4.54.1 (a814ff5)";
const HELP: &str = include_str!("../../../bin/help.txt");

#[derive(Default)]
struct Config {
    options: EvalOptions,
    input_format: Option<Format>,
    output_format: Option<Format>,
    positional: Vec<String>,
    expression: Option<String>,
    from_file: Option<String>,
    inplace: bool,
    exit_status: bool,
    help: bool,
    version: bool,
}

fn boolean(flag: &str, value: Option<&str>) -> Result<bool, String> {
    match value {
        None | Some("true") => Ok(true),
        Some("false") => Ok(false),
        Some(value) => Err(format!(
            "invalid argument \"{value}\" for \"{flag}\" flag: invalid syntax"
        )),
    }
}

fn format(value: &str, direction: &str) -> Result<Option<Format>, String> {
    match value {
        "yaml" | "y" => Ok(Some(Format::Yaml)),
        "json" | "j" => Ok(Some(Format::Json)),
        "auto" | "a" => Ok(None),
        _ => {
            let aliases = "yaml|y|kyaml|ky|json|j|props|p|csv|c|tsv|t|xml|x|base64|base64url|uri|toml|hcl|h|shell|s|lua|l|ini|i";
            if aliases.split('|').any(|alias| alias == value) {
                Err(format!("no support for {direction} format"))
            } else {
                Err(format!("unknown format '{value}' please use [{aliases}]"))
            }
        }
    }
}

fn parse_args(args: &[String]) -> Result<Config, String> {
    let mut config = Config::default();
    let mut positional_only = false;
    let mut index = 0;
    while index < args.len() {
        let argument = &args[index];
        if argument == "--" && !positional_only {
            positional_only = true;
            index += 1;
            continue;
        }
        if positional_only || argument == "-" || !argument.starts_with('-') {
            config.positional.push(argument.clone());
            index += 1;
            continue;
        }
        let long = argument.starts_with("--");
        let text = &argument[if long { 2 } else { 1 }..];
        let mut offset = 0;
        loop {
            let equal = text[offset..].find('=').map(|i| offset + i);
            let name = if long {
                &text[..equal.unwrap_or(text.len())]
            } else {
                &text[offset..offset + text[offset..].chars().next().map_or(0, char::len_utf8)]
            };
            let spelling = format!("{}{name}", if long { "--" } else { "-" });
            let mut value = equal
                .filter(|i| *i == offset + name.len())
                .map(|i| &text[i + 1..]);
            if matches!(
                name,
                "s" | "split-exp"
                    | "split-exp-file"
                    | "f"
                    | "front-matter"
                    | "header-preprocess"
                    | "yaml-fix-merge-anchor-to-spec"
            ) {
                return Err(format!("{spelling} is not supported by this CLI"));
            }
            if matches!(
                name,
                "n" | "null-input"
                    | "r"
                    | "unwrapScalar"
                    | "N"
                    | "no-doc"
                    | "i"
                    | "inplace"
                    | "e"
                    | "exit-status"
                    | "M"
                    | "C"
                    | "no-colors"
                    | "colors"
                    | "h"
                    | "help"
                    | "V"
                    | "version"
            ) {
                let parsed = boolean(&spelling, value)?;
                match name {
                    "n" | "null-input" => config.options.null_input = parsed,
                    "r" | "unwrapScalar" => config.options.unwrap_scalar = Some(parsed),
                    "N" | "no-doc" => config.options.print_doc_separators = !parsed,
                    "i" | "inplace" => config.inplace = parsed,
                    "e" | "exit-status" => config.exit_status = parsed,
                    "h" | "help" => config.help = parsed,
                    "V" | "version" => config.version = parsed,
                    _ => {}
                }
            } else if matches!(
                name,
                "p" | "input-format"
                    | "o"
                    | "output-format"
                    | "I"
                    | "indent"
                    | "expression"
                    | "from-file"
            ) {
                if !long && value.is_none() && offset + name.len() < text.len() {
                    value = Some(&text[offset + name.len()..]);
                }
                if value.is_none() {
                    index += 1;
                    value = args.get(index).map(String::as_str);
                }
                let value = value.ok_or_else(|| format!("flag needs an argument: {spelling}"))?;
                match name {
                    "expression" => config.expression = Some(value.to_string()),
                    "from-file" => config.from_file = Some(value.to_string()),
                    "p" | "input-format" => config.input_format = format(value, "input")?,
                    "o" | "output-format" => config.output_format = format(value, "output")?,
                    _ => {
                        let number = value
                            .parse::<i64>()
                            .ok()
                            .filter(|n| n.unsigned_abs() <= 9_007_199_254_740_991);
                        let number = number.ok_or_else(|| format!("invalid argument \"{value}\" for \"{spelling}\" flag: invalid syntax"))?;
                        if number < 0 {
                            return Err("indent must not be negative".into());
                        }
                        config.options.indent = number as usize;
                    }
                }
                break;
            } else {
                return Err(if long {
                    format!("unknown flag: {spelling}")
                } else {
                    format!("unknown shorthand flag: '{name}' in {argument}")
                });
            }
            if long || value.is_some() {
                break;
            }
            offset += name.len();
            if offset >= text.len() {
                break;
            }
        }
        index += 1;
    }
    match config.positional.first().map(String::as_str) {
        Some("eval" | "e") => {
            config.positional.remove(0);
        }
        Some("eval-all" | "ea") => {
            return Err(format!(
                "{} is not supported by this CLI",
                config.positional[0]
            ))
        }
        _ => {}
    }
    Ok(config)
}

fn read_text(path: &str) -> Result<String, String> {
    std::fs::read(path)
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                format!("open {path}: no such file or directory")
            } else {
                format!("open {path}: {error}")
            }
        })
}

fn skip_trivia(expression: &str, mut index: usize) -> usize {
    while let Some(character) = expression[index..].chars().next() {
        if character.is_whitespace() {
            index += character.len_utf8();
        } else if character == '#' {
            index = expression[index..]
                .find('\n')
                .map_or(expression.len(), |i| index + i + 1);
        } else {
            break;
        }
    }
    index
}

fn string_end(expression: &str, start: usize) -> usize {
    let mut index = start + 1;
    while index < expression.len() {
        let character = expression[index..].chars().next().unwrap();
        index += character.len_utf8();
        if character == '"' {
            break;
        }
        if character == '\\' {
            if let Some(escaped) = expression[index..].chars().next() {
                index += escaped.len_utf8();
            }
        }
    }
    index
}

fn preload_files(expression: &str) -> Result<Vec<(String, String)>, String> {
    let mut files = Vec::new();
    let mut index = 0;
    while index < expression.len() {
        index = skip_trivia(expression, index);
        let Some(character) = expression[index..].chars().next() else {
            break;
        };
        if character == '"' {
            index = string_end(expression, index);
            continue;
        }
        if !character.is_ascii_alphabetic() && character != '_' {
            index += character.len_utf8();
            continue;
        }
        let start = index;
        while index < expression.len()
            && (expression.as_bytes()[index].is_ascii_alphanumeric()
                || expression.as_bytes()[index] == b'_')
        {
            index += 1;
        }
        let name = &expression[start..index];
        let open = skip_trivia(expression, index);
        if !matches!(name, "load" | "load_str" | "load_props" | "load_base64")
            || (start > 0 && matches!(expression.as_bytes()[start - 1], b'.' | b'$'))
            || expression.as_bytes().get(open) != Some(&b'(')
        {
            continue;
        }
        let start = skip_trivia(expression, open + 1);
        let end = if expression.as_bytes().get(start) == Some(&b'"') {
            string_end(expression, start)
        } else {
            start
        };
        let path = serde_json::from_str::<String>(&expression[start..end]).ok();
        if path.is_none() || expression.as_bytes().get(skip_trivia(expression, end)) != Some(&b')')
        {
            return Err(format!("{name} requires a string-literal path in this CLI; dynamic load paths are not supported"));
        }
        let path = path.unwrap();
        if let Ok(text) = read_text(&path) {
            files.push((path, text));
        }
        index = end;
    }
    Ok(files)
}

fn now() -> String {
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let seconds = elapsed.as_secs();
    let mut days = seconds / 86400;
    let mut year = 1970 + 400 * (days / 146097);
    days %= 146097;
    let leap = |y: u64| y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400));
    loop {
        let length = if leap(year) { 366 } else { 365 };
        if days < length {
            break;
        }
        days -= length;
        year += 1;
    }
    let months = [
        31,
        if leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 0;
    while days >= months[month] {
        days -= months[month];
        month += 1;
    }
    format!(
        "{year:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        month + 1,
        days + 1,
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60,
        elapsed.subsec_millis()
    )
}

fn write_stdout(text: &str) -> Result<(), String> {
    std::io::stdout()
        .lock()
        .write_all(text.as_bytes())
        .map_err(|error| {
            if error.kind() == std::io::ErrorKind::BrokenPipe {
                std::process::exit(141);
            }
            error.to_string()
        })
}

fn run() -> Result<(), String> {
    let mut config = parse_args(&std::env::args().skip(1).collect::<Vec<_>>())?;
    if config.help
        || (config.positional.first().is_some_and(|s| s == "help")
            && config.expression.is_none()
            && config.from_file.is_none())
    {
        return write_stdout(&HELP.replace("{version}", env!("CARGO_PKG_VERSION")));
    }
    if config.version {
        return write_stdout(&format!(
            "yq {} (Go yq {REFERENCE})\n",
            env!("CARGO_PKG_VERSION")
        ));
    }
    if let Some(path) = config.from_file.as_ref() {
        config.expression = Some(read_text(path)?);
    }
    if config.expression.is_none() {
        let first = config.positional.first();
        let is_file = first.is_some_and(|path| path == "-" || std::path::Path::new(path).is_file());
        config.expression = Some(if is_file || first.is_none() {
            ".".into()
        } else {
            config.positional.remove(0)
        });
    }
    let expression = config.expression.unwrap();
    let paths = config.positional;
    if config.inplace && (paths.is_empty() || paths[0] == "-") {
        return Err(
            "write in place flag only applicable when giving an expression and at least one file"
                .into(),
        );
    }
    if config.options.null_input && !paths.is_empty() {
        return Err("cannot pass files in when using null-input flag".into());
    }
    config.options.env = std::env::vars_os()
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value.into_string().ok()?)))
        .collect();
    config.options.now = Some(now());
    config.options.files = preload_files(&expression)?;
    let detected = if paths
        .first()
        .and_then(|path| std::path::Path::new(path).extension())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
    {
        Format::Json
    } else {
        Format::Yaml
    };
    config.options.input_format = config.input_format.unwrap_or(detected);
    config.options.output_format =
        config
            .output_format
            .unwrap_or(if config.input_format.is_some() {
                Format::Yaml
            } else {
                detected
            });
    let mut inputs = Vec::new();
    let mut input_error = None;
    let mut stdin_consumed = false;
    let stdin = ["-".to_string()];
    let read_paths = if paths.is_empty() {
        &stdin[..]
    } else {
        &paths[..]
    };
    if !config.options.null_input {
        for path in read_paths {
            let result = if path == "-" {
                if stdin_consumed {
                    Ok(String::new())
                } else {
                    stdin_consumed = true;
                    let mut bytes = Vec::new();
                    std::io::stdin()
                        .read_to_end(&mut bytes)
                        .map(|_| String::from_utf8_lossy(&bytes).into_owned())
                        .map_err(|error| error.to_string())
                }
            } else {
                read_text(path)
            };
            match result {
                Ok(input) => inputs.push(input),
                Err(error) => {
                    input_error = Some(error);
                    break;
                }
            }
        }
    }
    let inputs: Vec<&str> = inputs.iter().map(String::as_str).collect();
    let result = match yq_core::evaluate_sequence_with_status(&expression, &inputs, &config.options)
    {
        Ok(result) => result,
        Err(failure) => {
            if !config.inplace {
                write_stdout(&failure.output)?;
            }
            return Err(failure.error.0);
        }
    };
    let failure = input_error
        .or_else(|| (config.exit_status && !result.matched).then(|| "no matches found".into()));
    if let Some(error) = failure {
        if !config.inplace {
            write_stdout(&result.output)?;
        }
        return Err(error);
    }
    if config.inplace {
        std::fs::write(&paths[0], result.output).map_err(|error| error.to_string())
    } else {
        write_stdout(&result.output)
    }
}

fn main() {
    if let Err(error) = run() {
        let error = if let Some((position, text)) = error.split_once(": invalid input text ") {
            if position.chars().all(|c| c.is_ascii_digit() || c == ':') {
                format!("{position}: lexer: invalid input text {text}")
            } else {
                error
            }
        } else {
            error
        };
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}
