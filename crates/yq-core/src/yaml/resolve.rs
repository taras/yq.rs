//! Tag resolution for plain scalars, ported from go-yaml's `resolver.go`.

/// `resolve("", value)`: the tag an untagged plain scalar gets.
pub fn resolve_tag(value: &str) -> &'static str {
    let hint = match value.as_bytes().first() {
        None => b'N',
        Some(first) => resolve_hint(*first),
    };
    if hint == 0 {
        return "!!str";
    }
    if let Some(tag) = resolve_map(value) {
        return tag;
    }
    match hint {
        b'M' => "!!str",
        b'.' => {
            if go_parse_float(value).is_some() {
                "!!float"
            } else {
                "!!str"
            }
        }
        b'D' | b'S' => {
            if parse_timestamp(value) {
                return "!!timestamp";
            }
            let plain = value.replace('_', "");
            if go_parse_int_base0(&plain) {
                return "!!int";
            }
            if is_yaml_style_float(&plain) && go_parse_float(&plain).is_some() {
                return "!!float";
            }
            "!!str"
        }
        _ => "!!str",
    }
}

fn resolve_hint(first: u8) -> u8 {
    match first {
        b'+' | b'-' => b'S',
        b'0'..=b'9' => b'D',
        b'y' | b'Y' | b'n' | b'N' | b't' | b'T' | b'f' | b'F' | b'o' | b'O' | b'~' | b'<' => b'M',
        b'.' => b'.',
        _ => 0,
    }
}

fn resolve_map(value: &str) -> Option<&'static str> {
    match value {
        "true" | "True" | "TRUE" | "false" | "False" | "FALSE" => Some("!!bool"),
        "" | "~" | "null" | "Null" | "NULL" => Some("!!null"),
        ".nan" | ".NaN" | ".NAN" | ".inf" | ".Inf" | ".INF" | "+.inf" | "+.Inf" | "+.INF"
        | "-.inf" | "-.Inf" | "-.INF" | "-0" | "-0.0" => Some("!!float"),
        "<<" => Some("!!merge"),
        _ => None,
    }
}

/// Go's `strconv.ParseInt(s, 0, 64)` or `ParseUint(s, 0, 64)` success test.
fn go_parse_int_base0(text: &str) -> bool {
    let (negative, digits) = match text.as_bytes().first() {
        Some(b'+') => (false, &text[1..]),
        Some(b'-') => (true, &text[1..]),
        _ => (false, text),
    };
    if digits.is_empty() {
        return false;
    }
    let lower = digits.to_ascii_lowercase();
    let (radix, body) = if let Some(rest) = lower.strip_prefix("0x") {
        (16, rest)
    } else if let Some(rest) = lower.strip_prefix("0o") {
        (8, rest)
    } else if let Some(rest) = lower.strip_prefix("0b") {
        (2, rest)
    } else if lower.len() > 1 && lower.starts_with('0') {
        (8, &lower[1..])
    } else {
        (10, lower.as_str())
    };
    if body.is_empty() || !body.bytes().all(|b| (b as char).is_digit(radix)) {
        return false;
    }
    if negative {
        return i128::from_str_radix(body, radix)
            .map(|v| -v >= i64::MIN as i128)
            .unwrap_or(false);
    }
    u64::from_str_radix(body, radix).is_ok()
}

fn is_yaml_style_float(text: &str) -> bool {
    // ^[-+]?(?:\.[0-9]+|[0-9]+(?:\.[0-9]*)?)(?:[eE][-+]?[0-9]+)?$
    let bytes = text.as_bytes();
    let mut i = 0;
    if i < bytes.len() && (bytes[i] == b'-' || bytes[i] == b'+') {
        i += 1;
    }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return false;
        }
    } else {
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return false;
        }
        if i < bytes.len() && bytes[i] == b'.' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
        }
    }
    if i < bytes.len() && (bytes[i] == b'e' || bytes[i] == b'E') {
        i += 1;
        if i < bytes.len() && (bytes[i] == b'-' || bytes[i] == b'+') {
            i += 1;
        }
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return false;
        }
    }
    i == bytes.len()
}

/// Go's `strconv.ParseFloat(s, 64)`: decimal and hex floats, `inf`, `nan`,
/// underscores only between digits.
pub fn go_parse_float(text: &str) -> Option<f64> {
    let lower = text.to_ascii_lowercase();
    let unsigned = lower.trim_start_matches(['+', '-']);
    if lower.len() - unsigned.len() > 1 {
        return None;
    }
    let negative = lower.starts_with('-');
    let special = match unsigned {
        "inf" | "infinity" => Some(f64::INFINITY),
        "nan" => Some(f64::NAN),
        _ => None,
    };
    if let Some(value) = special {
        return Some(if negative { -value } else { value });
    }
    if unsigned.is_empty() {
        return None;
    }
    let cleaned = if unsigned.contains('_') {
        if !underscores_ok(unsigned) {
            return None;
        }
        unsigned.replace('_', "")
    } else {
        unsigned.to_string()
    };
    if cleaned.starts_with("0x") {
        return None;
    }
    let bytes = cleaned.as_bytes();
    let mut i = 0;
    let mut digits = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
        digits += 1;
    }
    if i < bytes.len() && bytes[i] == b'.' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return None;
    }
    if i < bytes.len() && (bytes[i] == b'e') {
        i += 1;
        if i < bytes.len() && (bytes[i] == b'-' || bytes[i] == b'+') {
            i += 1;
        }
        let start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == start {
            return None;
        }
    }
    if i != bytes.len() {
        return None;
    }
    let value: f64 = cleaned.parse().ok()?;
    Some(if negative { -value } else { value })
}

fn underscores_ok(text: &str) -> bool {
    let bytes = text.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'_' {
            let prev_digit = i > 0 && bytes[i - 1].is_ascii_digit();
            let next_digit = i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit();
            if !prev_digit || !next_digit {
                return false;
            }
        }
    }
    true
}

/// `parseTimestamp`: the subset of YAML timestamps yaml.v3 recognises through
/// Go's `time.Parse` with its four layouts.
pub fn parse_timestamp(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i != 4 || i == bytes.len() || bytes[i] != b'-' {
        return false;
    }
    let date_end = match parse_date_prefix(bytes) {
        Some(end) => end,
        None => return false,
    };
    if date_end == bytes.len() {
        return true;
    }
    let separator = bytes[date_end];
    let with_zone = match separator {
        b'T' | b't' => true,
        b' ' => false,
        _ => return false,
    };
    let time_end = match parse_time(bytes, date_end + 1) {
        Some(end) => end,
        None => return false,
    };
    if !with_zone {
        return time_end == bytes.len();
    }
    parse_zone(bytes, time_end)
}

fn parse_number(bytes: &[u8], start: usize, min: usize, max: usize) -> Option<(u32, usize)> {
    let mut i = start;
    let mut value = 0u32;
    while i < bytes.len() && bytes[i].is_ascii_digit() && i - start < max {
        value = value * 10 + (bytes[i] - b'0') as u32;
        i += 1;
    }
    if i - start < min {
        return None;
    }
    Some((value, i))
}

fn parse_date_prefix(bytes: &[u8]) -> Option<usize> {
    let (year, i) = parse_number(bytes, 0, 4, 4)?;
    if i >= bytes.len() || bytes[i] != b'-' {
        return None;
    }
    let (month, i) = parse_number(bytes, i + 1, 1, 2)?;
    if i >= bytes.len() || bytes[i] != b'-' {
        return None;
    }
    let (day, i) = parse_number(bytes, i + 1, 1, 2)?;
    if !(1..=12).contains(&month) || day < 1 || day > days_in_month(year, month) {
        return None;
    }
    Some(i)
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            let leap =
                (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400);
            if leap {
                29
            } else {
                28
            }
        }
    }
}

fn parse_time(bytes: &[u8], start: usize) -> Option<usize> {
    let (hour, i) = parse_number(bytes, start, 2, 2)?;
    if i >= bytes.len() || bytes[i] != b':' {
        return None;
    }
    let (minute, i) = parse_number(bytes, i + 1, 1, 2)?;
    if i >= bytes.len() || bytes[i] != b':' {
        return None;
    }
    let (second, mut i) = parse_number(bytes, i + 1, 1, 2)?;
    if hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    if i < bytes.len() && bytes[i] == b'.' {
        let mut j = i + 1;
        while j < bytes.len() && bytes[j].is_ascii_digit() {
            j += 1;
        }
        if j > i + 1 {
            i = j;
        }
    }
    Some(i)
}

fn parse_zone(bytes: &[u8], start: usize) -> bool {
    if start >= bytes.len() {
        return false;
    }
    if bytes[start] == b'Z' {
        return start + 1 == bytes.len();
    }
    if bytes[start] != b'+' && bytes[start] != b'-' {
        return false;
    }
    let rest = &bytes[start + 1..];
    rest.len() == 5
        && rest[0].is_ascii_digit()
        && rest[1].is_ascii_digit()
        && rest[2] == b':'
        && rest[3].is_ascii_digit()
        && rest[4].is_ascii_digit()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_core_types() {
        assert_eq!(resolve_tag("12"), "!!int");
        assert_eq!(resolve_tag("0x1A"), "!!int");
        assert_eq!(resolve_tag("1_000"), "!!int");
        assert_eq!(resolve_tag("0755"), "!!int");
        assert_eq!(resolve_tag("08"), "!!float");
        assert_eq!(resolve_tag("1.5"), "!!float");
        assert_eq!(resolve_tag("1e3"), "!!float");
        assert_eq!(resolve_tag(".5"), "!!float");
        assert_eq!(resolve_tag("true"), "!!bool");
        assert_eq!(resolve_tag("yes"), "!!str");
        assert_eq!(resolve_tag("~"), "!!null");
        assert_eq!(resolve_tag(""), "!!null");
        assert_eq!(resolve_tag("<<"), "!!merge");
        assert_eq!(resolve_tag("2021-05-19"), "!!timestamp");
        assert_eq!(resolve_tag("2021-05-19T01:02:03Z"), "!!timestamp");
        assert_eq!(resolve_tag("2021-05-19T01:02:03.000000004Z"), "!!timestamp");
        assert_eq!(resolve_tag("2021-05-19 01:02:03"), "!!timestamp");
        assert_eq!(resolve_tag("1-2"), "!!str");
        assert_eq!(resolve_tag("cat"), "!!str");
        assert_eq!(resolve_tag("-"), "!!str");
        assert_eq!(resolve_tag("+1"), "!!int");
    }
}
