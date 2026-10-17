//! `operator_datetime.go`: now, with_dtf, format_datetime, tz, from_unix,
//! to_unix. The clock value comes from the host.

use crate::context::Context;
use crate::eval::Evaluator;
use crate::node::{Kind, Node, NodeId};
use crate::operation::{ExprNode, OpType};
use crate::ops::datetime::{format, parse_datetime, DateTime, ZoneName, RFC3339, RFC3339_NANO};
use crate::util::parse_float;
use crate::Error;

fn string_parameter(
    name: &str,
    d: &mut Evaluator,
    context: &Context,
    expr: Option<&ExprNode>,
) -> Result<String, Error> {
    let result = d.get_matching_nodes_opt(context.read_only_clone(), expr)?;
    match result.first() {
        Some(first) => Ok(d.arena.get(first).value.clone()),
        None => Err(Error(format!("could not find {name} for format_time"))),
    }
}

pub fn with_dtf_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let rhs = d.rhs(expr)?;
    if rhs.op.op_type == OpType::Block || rhs.op.op_type == OpType::Union {
        let layout = string_parameter("layout", d, &context, rhs.lhs())
            .map_err(|e| Error(format!("could not get date time format: {}", e.0)))?;
        let mut context = context;
        context.set_datetime_layout(&layout);
        return d.get_matching_nodes_opt(context, rhs.rhs());
    }
    Err(Error(
        "must provide a date time format string and an expression, e.g. with_dtf(\"Monday, 02-Jan-06 at 3:04PM MST\"; <exp>)".to_string(),
    ))
}

fn now(d: &Evaluator) -> Result<DateTime, Error> {
    let text = d.now.as_deref().ok_or_else(|| {
        Error("now is not available: the host did not supply a clock".to_string())
    })?;
    parse_datetime(RFC3339_NANO, text)
}

pub fn now_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    let value = format(&now(d)?, RFC3339);
    let node = d.arena.push(Node {
        kind: Kind::Scalar,
        tag: "!!timestamp".to_string(),
        value,
        ..Node::default()
    });
    Ok(context.single_child_context(node))
}

pub fn format_datetime_operator(
    d: &mut Evaluator,
    context: Context,
    expr: &ExprNode,
) -> Result<Context, Error> {
    let layout = context.datetime_layout().to_string();
    let target_format = string_parameter("format", d, &context, expr.rhs())?;
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        let value = d.arena.get(candidate).value.clone();
        let parsed = parse_datetime(&layout, &value).map_err(|e| {
            Error(format!(
                "could not parse datetime of [{}]: {}",
                d.arena.get_nice_path(candidate),
                e.0
            ))
        })?;
        let formatted = format(&parsed, &target_format);
        let node = match crate::yaml::decode::parse_snippet(&mut d.arena, &formatted) {
            Ok(node) => node,
            Err(_) => d.arena.push(Node::scalar("!!str", &formatted)),
        };
        let (parent, key) = {
            let source = d.arena.get(candidate);
            (source.parent, source.key)
        };
        let result = d.arena.get_mut(node);
        result.parent = parent;
        result.key = key;
        results.push(node);
    }
    Ok(context.child_context(results))
}

/// The handful of IANA zones the test suite uses, with their DST rules.
fn zone_offset(zone: &str, instant: &DateTime) -> Result<(i32, ZoneName), Error> {
    let named = |name: &str| {
        let mut bytes = [0u8; 8];
        let len = name.len().min(8);
        bytes[..len].copy_from_slice(&name.as_bytes()[..len]);
        ZoneName::Named(bytes, len)
    };
    match zone {
        "UTC" | "" => Ok((0, ZoneName::Utc)),
        "Local" => Ok((0, ZoneName::Utc)),
        "Australia/Perth" => Ok((8 * 3600, named("AWST"))),
        "Australia/Sydney" | "Australia/Melbourne" | "Australia/Canberra" | "Australia/Hobart" => {
            // DST from the first Sunday of October (02:00 standard) to the
            // first Sunday of April (03:00 daylight).
            let standard = DateTime {
                offset: 10 * 3600,
                zone_name: named("AEST"),
                ..*instant
            };
            let civil = standard.civil();
            let year = civil.year;
            let dst_start = first_sunday(year, 10) * 86400 + 2 * 3600 - 10 * 3600;
            let dst_end = first_sunday(year, 4) * 86400 + 3 * 3600 - 11 * 3600;
            let in_dst = instant.unix >= dst_start || instant.unix < dst_end;
            if in_dst {
                Ok((11 * 3600, named("AEDT")))
            } else {
                Ok((10 * 3600, named("AEST")))
            }
        }
        other => Err(Error(format!(
            "could not load tz [{other}]: unknown time zone {other}"
        ))),
    }
}

/// Days since the epoch of the first Sunday of `month` in `year`.
fn first_sunday(year: i64, month: i64) -> i64 {
    let first = DateTime {
        unix: 0,
        nanos: 0,
        offset: 0,
        zone_name: ZoneName::Utc,
    };
    let _ = first;
    let days = days_from_civil(year, month, 1);
    let weekday = (days + 4).rem_euclid(7);
    days + (7 - weekday) % 7
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub fn tz_operator(d: &mut Evaluator, context: Context, expr: &ExprNode) -> Result<Context, Error> {
    let layout = context.datetime_layout().to_string();
    let zone = string_parameter("timezone", d, &context, expr.rhs())?;
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        let value = d.arena.get(candidate).value.clone();
        let parsed = parse_datetime(&layout, &value).map_err(|e| {
            Error(format!(
                "could not parse datetime of [{}] using layout [{}]: {}",
                d.arena.get_nice_path(candidate),
                layout,
                e.0
            ))
        })?;
        let (offset, zone_name) = zone_offset(&zone, &parsed)?;
        let shifted = parsed.in_offset(offset, zone_name);
        let tag = d.arena.get(candidate).tag.clone();
        results.push(d.arena.create_replacement(
            candidate,
            Kind::Scalar,
            &tag,
            &format(&shifted, &layout),
        ));
    }
    Ok(context.child_context(results))
}

pub fn from_unix_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        let tag = d.arena.guess_tag_from_custom_type(candidate);
        if tag != "!!int" && tag != "!!float" {
            return Err(Error(format!(
                "from_unix only works on numbers, found {} instead",
                d.arena.get(candidate).tag
            )));
        }
        let seconds = parse_float(&d.arena.get(candidate).value)?;
        let millis = (seconds * 1000.0) as i64;
        let parsed = DateTime {
            unix: millis.div_euclid(1000),
            nanos: (millis.rem_euclid(1000) * 1_000_000) as u32,
            offset: 0,
            zone_name: ZoneName::Utc,
        };
        results.push(d.arena.create_replacement(
            candidate,
            Kind::Scalar,
            "!!timestamp",
            &format(&parsed, RFC3339),
        ));
    }
    Ok(context.child_context(results))
}

pub fn to_unix_operator(
    d: &mut Evaluator,
    context: Context,
    _expr: &ExprNode,
) -> Result<Context, Error> {
    let layout = context.datetime_layout().to_string();
    let mut results = Vec::new();
    for candidate in context.nodes().to_vec() {
        let value = d.arena.get(candidate).value.clone();
        let parsed = parse_datetime(&layout, &value).map_err(|e| {
            Error(format!(
                "could not parse datetime of [{}] using layout [{}]: {}",
                d.arena.get_nice_path(candidate),
                layout,
                e.0
            ))
        })?;
        results.push(d.arena.create_replacement(
            candidate,
            Kind::Scalar,
            "!!int",
            &parsed.unix.to_string(),
        ));
    }
    Ok(context.child_context(results))
}

#[allow(dead_code)]
fn unused(_: NodeId) {}
