//! Hand-written wasm ABI. No wasm-bindgen, so the module loads with
//! `WebAssembly.instantiate` alone in Deno, Bun, Node and browsers.
//! `docs/design.md` has the contract; `js/yq.ts` is the reference loader.
//!
//! Memory protocol:
//! - host calls `yq_alloc(len)` and writes UTF-8 bytes for each argument
//! - host calls `yq_eval(...)`, which returns a pointer to a 12-byte result
//!   header: status u32 (0 ok, 1 error), ptr u32, len u32 (little-endian)
//! - host copies the bytes out and calls `yq_result_free(header_ptr)`
//! - host frees its argument buffers with `yq_free(ptr, len)`

use yq_core::{EvalOptions, Format};

#[repr(C)]
pub struct ResultHeader {
    status: u32,
    ptr: u32,
    len: u32,
}

#[no_mangle]
pub extern "C" fn yq_alloc(len: u32) -> *mut u8 {
    let mut buffer = Vec::<u8>::with_capacity(len as usize);
    let pointer = buffer.as_mut_ptr();
    std::mem::forget(buffer);
    pointer
}

/// # Safety
/// `ptr` and `len` must come from a matching `yq_alloc` call.
#[no_mangle]
pub unsafe extern "C" fn yq_free(ptr: *mut u8, len: u32) {
    drop(Vec::from_raw_parts(ptr, 0, len as usize));
}

/// # Safety
/// All pointers must come from `yq_alloc` and hold valid UTF-8 of the given length.
#[no_mangle]
pub unsafe extern "C" fn yq_eval(
    expr_ptr: *const u8,
    expr_len: u32,
    input_ptr: *const u8,
    input_len: u32,
    opts_ptr: *const u8,
    opts_len: u32,
) -> *mut ResultHeader {
    let expression = read_str(expr_ptr, expr_len);
    let input = read_str(input_ptr, input_len);
    let options_json = read_str(opts_ptr, opts_len);

    let outcome = match parse_options(options_json) {
        Ok(options) => yq_core::evaluate(expression, input, &options).map_err(|e| e.0),
        Err(message) => Err(message),
    };
    let (status, text) = match outcome {
        Ok(text) => (0, text),
        Err(message) => (1, message),
    };
    let mut bytes = text.into_bytes();
    bytes.shrink_to_fit();
    let header = Box::new(ResultHeader {
        status,
        ptr: bytes.as_ptr() as u32,
        len: bytes.len() as u32,
    });
    std::mem::forget(bytes);
    Box::into_raw(header)
}

/// # Safety
/// `header` must come from `yq_eval` and not have been freed already.
#[no_mangle]
pub unsafe extern "C" fn yq_result_free(header: *mut ResultHeader) {
    let header = Box::from_raw(header);
    drop(Vec::from_raw_parts(
        header.ptr as *mut u8,
        header.len as usize,
        header.len as usize,
    ));
}

unsafe fn read_str<'a>(ptr: *const u8, len: u32) -> &'a str {
    if ptr.is_null() || len == 0 {
        return "";
    }
    std::str::from_utf8_unchecked(std::slice::from_raw_parts(ptr, len as usize))
}

/// Options arrive as a tiny JSON object. A dedicated parser keeps serde out of
/// the wasm binary; the keys are documented in `docs/design.md`.
fn parse_options(json: &str) -> Result<EvalOptions, String> {
    let mut options = EvalOptions::default();
    let trimmed = json.trim();
    if trimmed.is_empty() || trimmed == "{}" {
        return Ok(options);
    }
    for (key, value) in flat_json_pairs(trimmed)? {
        match key.as_str() {
            "inputFormat" => options.input_format = parse_format(&value)?,
            "outputFormat" => options.output_format = parse_format(&value)?,
            "indent" => {
                options.indent = value.parse().map_err(|_| format!("bad indent: {value}"))?
            }
            "nullInput" => options.null_input = value == "true",
            "now" => options.now = Some(value),
            "env" => options.env = flat_json_pairs(&value)?,
            "files" => options.files = flat_json_pairs(&value)?,
            other => return Err(format!("unknown option: {other}")),
        }
    }
    Ok(options)
}

fn parse_format(value: &str) -> Result<Format, String> {
    match value {
        "yaml" | "y" => Ok(Format::Yaml),
        "json" | "j" => Ok(Format::Json),
        other => Err(format!("unknown format: {other}")),
    }
}

/// Parses a one-level JSON object of string/number/bool/object values into
/// `(key, raw value)` pairs. Nested objects are returned as their JSON text.
fn flat_json_pairs(json: &str) -> Result<Vec<(String, String)>, String> {
    let body = json
        .trim()
        .strip_prefix('{')
        .and_then(|s| s.strip_suffix('}'))
        .ok_or_else(|| format!("options must be a JSON object, got: {json}"))?;
    let mut pairs = Vec::new();
    let chars: Vec<char> = body.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        while i < chars.len() && (chars[i].is_whitespace() || chars[i] == ',') {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        let key = read_json_string(&chars, &mut i)?;
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() || chars[i] != ':' {
            return Err(format!("expected ':' after key {key}"));
        }
        i += 1;
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        let value = if chars[i] == '"' {
            read_json_string(&chars, &mut i)?
        } else if chars[i] == '{' {
            let start = i;
            let mut depth = 0;
            loop {
                if chars[i] == '{' {
                    depth += 1;
                } else if chars[i] == '}' {
                    depth -= 1;
                    if depth == 0 {
                        i += 1;
                        break;
                    }
                }
                i += 1;
                if i >= chars.len() {
                    return Err("unterminated object in options".to_string());
                }
            }
            chars[start..i].iter().collect()
        } else {
            let start = i;
            while i < chars.len() && chars[i] != ',' && !chars[i].is_whitespace() {
                i += 1;
            }
            chars[start..i].iter().collect()
        };
        pairs.push((key, value));
    }
    Ok(pairs)
}

fn read_json_string(chars: &[char], i: &mut usize) -> Result<String, String> {
    if chars[*i] != '"' {
        return Err("expected string".to_string());
    }
    *i += 1;
    let mut out = String::new();
    while *i < chars.len() {
        match chars[*i] {
            '"' => {
                *i += 1;
                return Ok(out);
            }
            '\\' => {
                *i += 1;
                let escaped = chars.get(*i).ok_or("bad escape")?;
                match escaped {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    'u' => {
                        let hex: String = chars
                            .get(*i + 1..*i + 5)
                            .ok_or("bad \\u escape")?
                            .iter()
                            .collect();
                        let code = u32::from_str_radix(&hex, 16).map_err(|_| "bad \\u escape")?;
                        out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                        *i += 4;
                    }
                    other => out.push(*other),
                }
            }
            c => out.push(c),
        }
        *i += 1;
    }
    Err("unterminated string".to_string())
}
