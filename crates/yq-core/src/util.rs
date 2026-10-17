//! Helpers from `lib.go`: number parsing, escapes, comment text.

use crate::node::Node;
use crate::Error;

/// `parseInt64`: yaml integers may carry underscores and `0x`/`0o` prefixes.
/// Returns the Go format verb used to print the result alongside the value.
pub fn parse_int64(number_string: &str) -> Result<(&'static str, i64), Error> {
    let cleaned: String = if number_string.contains('_') {
        number_string.replace('_', "")
    } else {
        number_string.to_string()
    };
    let (sign, digits) = match cleaned.as_bytes().first() {
        Some(b'+') => ("+", &cleaned[1..]),
        Some(b'-') => ("-", &cleaned[1..]),
        _ => ("", cleaned.as_str()),
    };
    let bad = || {
        Error(format!(
            "strconv.ParseInt: parsing \"{number_string}\": invalid syntax"
        ))
    };
    if let Some(hex) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        let value = i64::from_str_radix(&format!("{sign}{hex}"), 16).map_err(|_| bad())?;
        return Ok(("0x%X", value));
    }
    if let Some(octal) = digits.strip_prefix("0o") {
        let value = i64::from_str_radix(&format!("{sign}{octal}"), 8).map_err(|_| bad())?;
        return Ok(("0o%o", value));
    }
    let trimmed = cleaned.strip_prefix('+').unwrap_or(&cleaned);
    if trimmed.is_empty()
        || !trimmed
            .trim_start_matches('-')
            .bytes()
            .all(|b| b.is_ascii_digit())
    {
        return Err(bad());
    }
    let value = trimmed.parse::<i64>().map_err(|_| {
        Error(format!(
            "strconv.ParseInt: parsing \"{number_string}\": value out of range"
        ))
    })?;
    Ok(("%v", value))
}

pub fn parse_int(number_string: &str) -> Result<i64, Error> {
    Ok(parse_int64(number_string)?.1)
}

/// Go's `strconv.ParseFloat` for the inputs yq feeds it.
pub fn parse_float(text: &str) -> Result<f64, Error> {
    let cleaned = text.replace('_', "");
    let lower = cleaned.to_ascii_lowercase();
    let special = match lower.trim_start_matches('+') {
        "inf" | "infinity" | ".inf" => Some(f64::INFINITY),
        "-inf" | "-infinity" | "-.inf" => Some(f64::NEG_INFINITY),
        "nan" | ".nan" => Some(f64::NAN),
        _ => None,
    };
    if let Some(value) = special {
        return Ok(value);
    }
    let valid = !lower.is_empty()
        && lower
            .bytes()
            .all(|b| b.is_ascii_digit() || matches!(b, b'.' | b'e' | b'-' | b'+'));
    if !valid {
        return Err(Error(format!(
            "strconv.ParseFloat: parsing \"{text}\": invalid syntax"
        )));
    }
    cleaned.parse::<f64>().map_err(|_| {
        Error(format!(
            "strconv.ParseFloat: parsing \"{text}\": invalid syntax"
        ))
    })
}

/// Formats a float the way Go's `%v` does for float64.
pub fn format_float(value: f64) -> String {
    if value.is_nan() {
        return "NaN".to_string();
    }
    if value.is_infinite() {
        return if value > 0.0 { "+Inf" } else { "-Inf" }.to_string();
    }
    if value == 0.0 {
        return if value.is_sign_negative() { "-0" } else { "0" }.to_string();
    }
    let exponent = value.abs().log10().floor() as i32;
    if !(-4..21).contains(&exponent) {
        // Go prints %e with the shortest mantissa and a two-digit exponent.
        let formatted = format!("{value:e}");
        let (mantissa, exp) = formatted.split_once('e').unwrap_or((&formatted, "0"));
        let exp_value: i32 = exp.parse().unwrap_or(0);
        let sign = if exp_value < 0 { '-' } else { '+' };
        return format!("{mantissa}e{sign}{:02}", exp_value.abs());
    }
    format!("{value}")
}

pub fn is_truthy_node(node: &Node) -> bool {
    if node.tag == "!!null" {
        return false;
    }
    if node.kind == crate::node::Kind::Scalar && node.tag == "!!bool" {
        let value = node.value.to_ascii_lowercase();
        return matches!(value.as_str(), "y" | "yes" | "on" | "true");
    }
    true
}

/// `checkedInt64Arithmetic`: refuses to wrap on overflow.
pub fn checked_int64_arithmetic(op: char, lhs: i64, rhs: i64) -> Result<i64, Error> {
    let result = match op {
        '+' => lhs.checked_add(rhs),
        '-' => lhs.checked_sub(rhs),
        '*' => lhs.checked_mul(rhs),
        _ => return Err(Error(format!("unknown arithmetic operator '{op}'"))),
    };
    result.ok_or_else(|| Error(format!("{lhs} {op} {rhs} overflows int64")))
}

/// `processEscapeCharacters`: unescapes a double-quoted expression literal.
pub fn process_escape_characters(original: &str) -> String {
    if original.is_empty() {
        return String::new();
    }
    let runes: Vec<char> = original.chars().collect();
    let mut result = String::new();
    let mut i = 0;
    while i < runes.len() {
        if runes[i] == '\\' && i + 1 < runes.len() {
            let next = runes[i + 1];
            let replacement = match next {
                '\\' => {
                    // `\\(` stays as two backslashes so string interpolation
                    // can see the escaped paren.
                    if i + 2 < runes.len() && runes[i + 2] == '(' {
                        result.push('\\');
                        result.push('\\');
                        i += 2;
                        continue;
                    }
                    Some('\\')
                }
                '"' => Some('"'),
                'n' => Some('\n'),
                't' => Some('\t'),
                'r' => Some('\r'),
                'f' => Some('\u{000C}'),
                'v' => Some('\u{000B}'),
                'b' => Some('\u{0008}'),
                'a' => Some('\u{0007}'),
                _ => None,
            };
            if let Some(ch) = replacement {
                result.push(ch);
                i += 2;
                continue;
            }
        }
        result.push(runes[i]);
        i += 1;
    }
    result
}

pub fn head_comment(node: &Node) -> String {
    node.head_comment.replacen('#', "", 1)
}

pub fn line_comment(node: &Node) -> String {
    node.line_comment.replacen('#', "", 1)
}

pub fn foot_comment(node: &Node) -> String {
    node.foot_comment.replacen('#', "", 1)
}

pub fn head_and_line_comment(node: &Node) -> String {
    head_comment(node) + &line_comment(node)
}
