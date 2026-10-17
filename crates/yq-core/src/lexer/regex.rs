//! A small backtracking regex matcher for the lexer's rule table. It covers
//! exactly the syntax those patterns use (classes, groups, alternation and
//! the `* + ?` quantifiers) with leftmost-first semantics, which is what Go's
//! `regexp` gives participle when it anchors each rule at the current
//! position. Keeping the Go patterns verbatim matters more than speed here.

#[derive(Debug, Clone)]
pub enum Re {
    Char(char),
    Any,
    Class {
        ranges: Vec<(char, char)>,
        negated: bool,
    },
    Group(Vec<Vec<Re>>),
    Repeat {
        inner: Vec<Re>,
        min: usize,
        max: Option<usize>,
    },
}

pub fn compile(pattern: &str) -> Vec<Re> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut pos = 0;
    let alternatives = parse_alternation(&chars, &mut pos);
    if alternatives.len() == 1 {
        alternatives.into_iter().next().unwrap_or_default()
    } else {
        vec![Re::Group(alternatives)]
    }
}

fn parse_alternation(chars: &[char], pos: &mut usize) -> Vec<Vec<Re>> {
    let mut alternatives = vec![parse_sequence(chars, pos)];
    while *pos < chars.len() && chars[*pos] == '|' {
        *pos += 1;
        alternatives.push(parse_sequence(chars, pos));
    }
    alternatives
}

fn parse_sequence(chars: &[char], pos: &mut usize) -> Vec<Re> {
    let mut items = Vec::new();
    while *pos < chars.len() {
        let c = chars[*pos];
        if c == '|' || c == ')' {
            break;
        }
        *pos += 1;
        let atom = match c {
            '(' => {
                let alternatives = parse_alternation(chars, pos);
                if *pos < chars.len() && chars[*pos] == ')' {
                    *pos += 1;
                }
                Re::Group(alternatives)
            }
            '[' => parse_class(chars, pos),
            '.' => Re::Any,
            '\\' => {
                let escaped = chars.get(*pos).copied().unwrap_or('\\');
                *pos += 1;
                escape_atom(escaped)
            }
            other => Re::Char(other),
        };
        let quantified = match chars.get(*pos) {
            Some('*') => {
                *pos += 1;
                Re::Repeat {
                    inner: vec![atom],
                    min: 0,
                    max: None,
                }
            }
            Some('+') => {
                *pos += 1;
                Re::Repeat {
                    inner: vec![atom],
                    min: 1,
                    max: None,
                }
            }
            Some('?') => {
                *pos += 1;
                Re::Repeat {
                    inner: vec![atom],
                    min: 0,
                    max: Some(1),
                }
            }
            _ => atom,
        };
        items.push(quantified);
    }
    items
}

fn escape_atom(escaped: char) -> Re {
    match escaped {
        's' => Re::Class {
            ranges: vec![
                (' ', ' '),
                ('\t', '\t'),
                ('\n', '\n'),
                ('\r', '\r'),
                ('\u{000C}', '\u{000C}'),
            ],
            negated: false,
        },
        'd' => Re::Class {
            ranges: vec![('0', '9')],
            negated: false,
        },
        'n' => Re::Char('\n'),
        't' => Re::Char('\t'),
        other => Re::Char(other),
    }
}

fn parse_class(chars: &[char], pos: &mut usize) -> Re {
    let mut negated = false;
    if chars.get(*pos) == Some(&'^') {
        negated = true;
        *pos += 1;
    }
    let mut ranges = Vec::new();
    let mut first = true;
    while *pos < chars.len() {
        let c = chars[*pos];
        if c == ']' && !first {
            *pos += 1;
            break;
        }
        first = false;
        *pos += 1;
        let start = if c == '\\' {
            let escaped = chars.get(*pos).copied().unwrap_or('\\');
            *pos += 1;
            match escaped {
                's' => {
                    ranges.extend([(' ', ' '), ('\t', '\t'), ('\n', '\n'), ('\r', '\r')]);
                    continue;
                }
                'd' => {
                    ranges.push(('0', '9'));
                    continue;
                }
                'n' => '\n',
                't' => '\t',
                other => other,
            }
        } else {
            c
        };
        if chars.get(*pos) == Some(&'-') && chars.get(*pos + 1).is_some_and(|c| *c != ']') {
            *pos += 1;
            let mut end = chars[*pos];
            *pos += 1;
            if end == '\\' {
                end = chars.get(*pos).copied().unwrap_or('\\');
                *pos += 1;
            }
            ranges.push((start, end));
        } else {
            ranges.push((start, start));
        }
    }
    Re::Class { ranges, negated }
}

/// Matches `items` at the start of `input` and returns the length (in chars)
/// of the leftmost-first match.
pub fn match_prefix(items: &[Re], input: &[char]) -> Option<usize> {
    match_here(items, input, 0, &|end| Some(end))
}

fn match_here(
    items: &[Re],
    input: &[char],
    pos: usize,
    k: &dyn Fn(usize) -> Option<usize>,
) -> Option<usize> {
    let first = match items.first() {
        None => return k(pos),
        Some(first) => first,
    };
    let rest = &items[1..];
    match first {
        Re::Char(c) => {
            if input.get(pos) == Some(c) {
                match_here(rest, input, pos + 1, k)
            } else {
                None
            }
        }
        Re::Any => {
            if pos < input.len() && input[pos] != '\n' {
                match_here(rest, input, pos + 1, k)
            } else {
                None
            }
        }
        Re::Class { ranges, negated } => {
            let c = *input.get(pos)?;
            let in_class = ranges.iter().any(|(lo, hi)| *lo <= c && c <= *hi);
            if in_class != *negated {
                match_here(rest, input, pos + 1, k)
            } else {
                None
            }
        }
        Re::Group(alternatives) => {
            for alternative in alternatives {
                let result =
                    match_here(alternative, input, pos, &|p| match_here(rest, input, p, k));
                if result.is_some() {
                    return result;
                }
            }
            None
        }
        Re::Repeat { inner, min, max } => match_repeat(inner, *min, *max, rest, input, pos, 0, k),
    }
}

#[allow(clippy::too_many_arguments)]
fn match_repeat(
    inner: &[Re],
    min: usize,
    max: Option<usize>,
    rest: &[Re],
    input: &[char],
    pos: usize,
    count: usize,
    k: &dyn Fn(usize) -> Option<usize>,
) -> Option<usize> {
    if max.is_none_or(|m| count < m) {
        let more = match_here(inner, input, pos, &|p| {
            if p == pos {
                None
            } else {
                match_repeat(inner, min, max, rest, input, p, count + 1, k)
            }
        });
        if more.is_some() {
            return more;
        }
    }
    if count >= min {
        return match_here(rest, input, pos, k);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(pattern: &str, input: &str) -> Option<usize> {
        let chars: Vec<char> = input.chars().collect();
        match_prefix(&compile(pattern), &chars)
    }

    #[test]
    fn matches_lexer_patterns() {
        assert_eq!(m(r"\.\[", ".[1]"), Some(2));
        assert_eq!(m(r"line_?comment|lineComment", "lineComment = 1"), Some(11));
        assert_eq!(m(r"line_?comment|lineComment", "line_comment"), Some(12));
        assert_eq!(m(r"\$[a-zA-Z_\-0-9]+", "$foo-bar | ."), Some(8));
        assert_eq!(m(r":\s*", ":  1"), Some(3));
        assert_eq!(m(r"flatten\([0-9]+\)", "flatten(12)"), Some(11));
        assert_eq!(m(r"-?\d+(\.\d+)", "-1.5e"), Some(4));
        assert_eq!(m(r"-?\d+(\.\d+)", "15"), None);
        assert_eq!(m(r"-?[1-9](\.\d+)?[Ee][-+]?\d+", "1e3"), Some(3));
        assert_eq!(
            m(r#""([^"\\]*(\\.[^"\\]*)*)""#, r#""a \"b\" c" + 1"#),
            Some(11)
        );
        assert_eq!(m(r"\s*==\s*", " == 3"), Some(4));
        assert_eq!(m(r"\.[^ ;\}\{\:\[\],\|\.\[\(\)=\n!]+\??", ".a.b"), Some(2));
        assert_eq!(
            m(r"\.[^ ;\}\{\:\[\],\|\.\[\(\)=\n!]+\??", ".a? | x"),
            Some(3)
        );
        assert_eq!(m(r#"\."[^ "]+"\??"#, r#"."a.b""#), Some(6));
        assert_eq!(m(r"\*=[\+|\?cdn]*", "*=+n"), Some(4));
        assert_eq!(m(r"[ \t\n]+", "  \n x"), Some(4));
        assert_eq!(m(r"#.*", "# hi\nx"), Some(4));
        assert_eq!(
            m(r"envsubst\((ne|nu|ff| |,)+\)", "envsubst(ne, nu)"),
            Some(16)
        );
        assert_eq!(m(r"strenv\([^\)]+\)", "strenv(FOO)"), Some(11));
        assert_eq!(m(r"[Nn][Uu][Ll][Ll]|~", "~"), Some(1));
        assert_eq!(m(r"0[xX][0-9A-Fa-f]+", "0x1F"), Some(4));
    }
}
