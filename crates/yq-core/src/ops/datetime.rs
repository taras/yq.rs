//! Go `time` layouts, parsing and formatting, enough for `operator_datetime.go`.
//! There is no clock here: `now` comes from the host through `EvalOptions`.

use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DateTime {
    /// Seconds since the Unix epoch, in UTC.
    pub unix: i64,
    pub nanos: u32,
    /// Offset from UTC in seconds, kept for formatting.
    pub offset: i32,
    pub zone_name: ZoneName,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ZoneName {
    Utc,
    Numeric,
    Named([u8; 8], usize),
}

pub const RFC3339: &str = "2006-01-02T15:04:05Z07:00";
pub const RFC3339_NANO: &str = "2006-01-02T15:04:05.999999999Z07:00";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layout {
    Literal(char),
    LongYear,
    ShortYear,
    ZeroMonth,
    Month,
    ShortMonthName,
    LongMonthName,
    ZeroDay,
    Day,
    UnderDay,
    YearDay,
    ShortWeekDay,
    LongWeekDay,
    Hour24,
    ZeroHour12,
    Hour12,
    ZeroMinute,
    Minute,
    ZeroSecond,
    Second,
    PmUpper,
    PmLower,
    Zone,
    IsoZoneColon,
    IsoZone,
    IsoZoneShort,
    NumZoneColon,
    NumZone,
    NumZoneShort,
    FracZeros(usize),
    FracNines(usize),
}

fn tokenize_layout(layout: &str) -> Vec<Layout> {
    let chars: Vec<char> = layout.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    let starts = |i: usize, s: &str| -> bool {
        let needle: Vec<char> = s.chars().collect();
        chars.len() >= i + needle.len() && chars[i..i + needle.len()] == needle[..]
    };
    while i < chars.len() {
        let candidates: &[(&str, Layout)] = &[
            ("2006", Layout::LongYear),
            ("January", Layout::LongMonthName),
            ("Jan", Layout::ShortMonthName),
            ("Monday", Layout::LongWeekDay),
            ("Mon", Layout::ShortWeekDay),
            ("MST", Layout::Zone),
            ("01", Layout::ZeroMonth),
            ("02", Layout::ZeroDay),
            ("002", Layout::YearDay),
            ("_2", Layout::UnderDay),
            ("15", Layout::Hour24),
            ("03", Layout::ZeroHour12),
            ("04", Layout::ZeroMinute),
            ("05", Layout::ZeroSecond),
            ("06", Layout::ShortYear),
            ("PM", Layout::PmUpper),
            ("pm", Layout::PmLower),
            ("Z07:00", Layout::IsoZoneColon),
            ("Z0700", Layout::IsoZone),
            ("Z07", Layout::IsoZoneShort),
            ("-07:00", Layout::NumZoneColon),
            ("-0700", Layout::NumZone),
            ("-07", Layout::NumZoneShort),
        ];
        let mut matched = false;
        if starts(i, "002") {
            tokens.push(Layout::YearDay);
            i += 3;
            continue;
        }
        for (text, token) in candidates {
            if starts(i, text) {
                tokens.push(*token);
                i += text.chars().count();
                matched = true;
                break;
            }
        }
        if matched {
            continue;
        }
        let c = chars[i];
        if (c == '.' || c == ',')
            && i + 1 < chars.len()
            && (chars[i + 1] == '0' || chars[i + 1] == '9')
        {
            let digit = chars[i + 1];
            let mut j = i + 1;
            while j < chars.len() && chars[j] == digit {
                j += 1;
            }
            let count = j - i - 1;
            if j >= chars.len() || !chars[j].is_ascii_digit() {
                tokens.push(if digit == '0' {
                    Layout::FracZeros(count)
                } else {
                    Layout::FracNines(count)
                });
                i = j;
                continue;
            }
        }
        match c {
            '1' => tokens.push(Layout::Month),
            '2' => tokens.push(Layout::Day),
            '3' => tokens.push(Layout::Hour12),
            '4' => tokens.push(Layout::Minute),
            '5' => tokens.push(Layout::Second),
            other => tokens.push(Layout::Literal(other)),
        }
        i += 1;
    }
    tokens
}

const SHORT_MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];
const LONG_MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const SHORT_DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const LONG_DAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if is_leap(year) {
                29
            } else {
                28
            }
        }
    }
}

pub struct Civil {
    pub year: i64,
    pub month: i64,
    pub day: i64,
    pub hour: i64,
    pub minute: i64,
    pub second: i64,
    pub nanos: u32,
    pub weekday: i64,
    pub year_day: i64,
}

impl DateTime {
    pub fn civil(&self) -> Civil {
        let local = self.unix + self.offset as i64;
        let days = local.div_euclid(86400);
        let secs = local.rem_euclid(86400);
        let (year, month, day) = civil_from_days(days);
        let weekday = (days + 4).rem_euclid(7);
        let year_day = days - days_from_civil(year, 1, 1) + 1;
        Civil {
            year,
            month,
            day,
            hour: secs / 3600,
            minute: (secs % 3600) / 60,
            second: secs % 60,
            nanos: self.nanos,
            weekday,
            year_day,
        }
    }

    pub fn add_nanos(&self, nanos: i128) -> DateTime {
        let total = self.unix as i128 * 1_000_000_000 + self.nanos as i128 + nanos;
        DateTime {
            unix: total.div_euclid(1_000_000_000) as i64,
            nanos: total.rem_euclid(1_000_000_000) as u32,
            offset: self.offset,
            zone_name: self.zone_name,
        }
    }

    pub fn in_offset(&self, offset: i32, zone_name: ZoneName) -> DateTime {
        DateTime {
            offset,
            zone_name,
            ..*self
        }
    }
}

fn parse_error(text: &str, layout: &str, rest: &str, element: &str) -> Error {
    Error(format!(
        "parsing time \"{text}\" as \"{layout}\": cannot parse \"{rest}\" as \"{element}\""
    ))
}

fn take_digits(input: &[char], pos: &mut usize, min: usize, max: usize) -> Option<i64> {
    let start = *pos;
    while *pos < input.len() && input[*pos].is_ascii_digit() && *pos - start < max {
        *pos += 1;
    }
    if *pos - start < min {
        *pos = start;
        return None;
    }
    input[start..*pos].iter().collect::<String>().parse().ok()
}

fn take_name(input: &[char], pos: &mut usize, names: &[&str]) -> Option<usize> {
    for (index, name) in names.iter().enumerate() {
        let chars: Vec<char> = name.chars().collect();
        if input.len() >= *pos + chars.len()
            && input[*pos..*pos + chars.len()]
                .iter()
                .zip(chars.iter())
                .all(|(a, b)| a.eq_ignore_ascii_case(b))
        {
            *pos += chars.len();
            return Some(index);
        }
    }
    None
}

/// Go's `time.Parse`.
pub fn parse_with_layout(layout: &str, text: &str) -> Result<DateTime, Error> {
    let tokens = tokenize_layout(layout);
    let input: Vec<char> = text.chars().collect();
    let mut pos = 0;
    let (mut year, mut month, mut day) = (1i64, 1i64, 1i64);
    let (mut hour, mut minute, mut second) = (0i64, 0i64, 0i64);
    let mut nanos = 0u32;
    let mut pm: Option<bool> = None;
    let mut offset: Option<i32> = None;
    let mut zone_name = ZoneName::Utc;
    let mut year_day: Option<i64> = None;
    let layout_text = |token: &Layout| -> String {
        match token {
            Layout::LongYear => "2006".into(),
            Layout::ShortYear => "06".into(),
            Layout::ZeroMonth => "01".into(),
            Layout::Month => "1".into(),
            Layout::ShortMonthName => "Jan".into(),
            Layout::LongMonthName => "January".into(),
            Layout::ZeroDay => "02".into(),
            Layout::Day => "2".into(),
            Layout::UnderDay => "_2".into(),
            Layout::YearDay => "002".into(),
            Layout::ShortWeekDay => "Mon".into(),
            Layout::LongWeekDay => "Monday".into(),
            Layout::Hour24 => "15".into(),
            Layout::ZeroHour12 => "03".into(),
            Layout::Hour12 => "3".into(),
            Layout::ZeroMinute => "04".into(),
            Layout::Minute => "4".into(),
            Layout::ZeroSecond => "05".into(),
            Layout::Second => "5".into(),
            Layout::PmUpper => "PM".into(),
            Layout::PmLower => "pm".into(),
            Layout::Zone => "MST".into(),
            Layout::IsoZoneColon => "Z07:00".into(),
            Layout::IsoZone => "Z0700".into(),
            Layout::IsoZoneShort => "Z07".into(),
            Layout::NumZoneColon => "-07:00".into(),
            Layout::NumZone => "-0700".into(),
            Layout::NumZoneShort => "-07".into(),
            Layout::FracZeros(n) => format!(".{}", "0".repeat(*n)),
            Layout::FracNines(n) => format!(".{}", "9".repeat(*n)),
            Layout::Literal(c) => c.to_string(),
        }
    };
    let fail = |pos: usize, token: &Layout| -> Error {
        let rest: String = input[pos.min(input.len())..].iter().collect();
        parse_error(text, layout, &rest, &layout_text(token))
    };
    let mut index = 0;
    while index < tokens.len() {
        let token = tokens[index];
        index += 1;
        match token {
            Layout::Literal(c) => {
                if input.get(pos) == Some(&c) {
                    pos += 1;
                } else {
                    return Err(fail(pos, &token));
                }
            }
            Layout::LongYear => {
                year = take_digits(&input, &mut pos, 4, 4).ok_or_else(|| fail(pos, &token))?;
            }
            Layout::ShortYear => {
                let short = take_digits(&input, &mut pos, 2, 2).ok_or_else(|| fail(pos, &token))?;
                year = if short >= 69 {
                    1900 + short
                } else {
                    2000 + short
                };
            }
            Layout::ZeroMonth => {
                month = take_digits(&input, &mut pos, 2, 2).ok_or_else(|| fail(pos, &token))?;
            }
            Layout::Month => {
                month = take_digits(&input, &mut pos, 1, 2).ok_or_else(|| fail(pos, &token))?;
            }
            Layout::ShortMonthName => {
                month = take_name(&input, &mut pos, &SHORT_MONTHS)
                    .ok_or_else(|| fail(pos, &token))? as i64
                    + 1;
            }
            Layout::LongMonthName => {
                month = take_name(&input, &mut pos, &LONG_MONTHS)
                    .ok_or_else(|| fail(pos, &token))? as i64
                    + 1;
            }
            Layout::ZeroDay => {
                day = take_digits(&input, &mut pos, 2, 2).ok_or_else(|| fail(pos, &token))?;
            }
            Layout::Day => {
                day = take_digits(&input, &mut pos, 1, 2).ok_or_else(|| fail(pos, &token))?;
            }
            Layout::UnderDay => {
                if input.get(pos) == Some(&' ') {
                    pos += 1;
                }
                day = take_digits(&input, &mut pos, 1, 2).ok_or_else(|| fail(pos, &token))?;
            }
            Layout::YearDay => {
                year_day =
                    Some(take_digits(&input, &mut pos, 3, 3).ok_or_else(|| fail(pos, &token))?);
            }
            Layout::ShortWeekDay => {
                take_name(&input, &mut pos, &SHORT_DAYS).ok_or_else(|| fail(pos, &token))?;
            }
            Layout::LongWeekDay => {
                take_name(&input, &mut pos, &LONG_DAYS).ok_or_else(|| fail(pos, &token))?;
            }
            Layout::Hour24 => {
                hour = take_digits(&input, &mut pos, 1, 2).ok_or_else(|| fail(pos, &token))?;
            }
            Layout::ZeroHour12 => {
                hour = take_digits(&input, &mut pos, 2, 2).ok_or_else(|| fail(pos, &token))?;
            }
            Layout::Hour12 => {
                hour = take_digits(&input, &mut pos, 1, 2).ok_or_else(|| fail(pos, &token))?;
            }
            Layout::ZeroMinute => {
                minute = take_digits(&input, &mut pos, 2, 2).ok_or_else(|| fail(pos, &token))?;
            }
            Layout::Minute => {
                minute = take_digits(&input, &mut pos, 1, 2).ok_or_else(|| fail(pos, &token))?;
            }
            Layout::ZeroSecond => {
                second = take_digits(&input, &mut pos, 2, 2).ok_or_else(|| fail(pos, &token))?;
                // Go accepts an unannounced fractional second after seconds.
                let fraction_follows = matches!(
                    tokens.get(index),
                    Some(Layout::FracZeros(_)) | Some(Layout::FracNines(_))
                );
                if !fraction_follows
                    && input.get(pos) == Some(&'.')
                    && input.get(pos + 1).is_some_and(|c| c.is_ascii_digit())
                {
                    pos += 1;
                    nanos = take_fraction(&input, &mut pos);
                }
            }
            Layout::Second => {
                second = take_digits(&input, &mut pos, 1, 2).ok_or_else(|| fail(pos, &token))?;
            }
            Layout::PmUpper | Layout::PmLower => {
                let marker =
                    take_name(&input, &mut pos, &["AM", "PM"]).ok_or_else(|| fail(pos, &token))?;
                pm = Some(marker == 1);
            }
            Layout::FracZeros(count) => {
                if input.get(pos) != Some(&'.') && input.get(pos) != Some(&',') {
                    return Err(fail(pos, &token));
                }
                pos += 1;
                let start = pos;
                nanos = take_fraction(&input, &mut pos);
                if pos - start != count {
                    return Err(fail(start, &token));
                }
            }
            Layout::FracNines(_) => {
                let separator = input.get(pos) == Some(&'.') || input.get(pos) == Some(&',');
                if separator && input.get(pos + 1).is_some_and(|c| c.is_ascii_digit()) {
                    pos += 1;
                    nanos = take_fraction(&input, &mut pos);
                }
            }
            Layout::Zone => {
                let start = pos;
                while pos < input.len() && input[pos].is_ascii_uppercase() {
                    pos += 1;
                }
                if pos - start < 3 {
                    return Err(fail(start, &token));
                }
                let name: String = input[start..pos].iter().collect();
                if name == "UTC" {
                    offset = Some(0);
                    zone_name = ZoneName::Utc;
                } else {
                    offset = Some(0);
                    zone_name = named_zone(&name);
                }
            }
            Layout::IsoZoneColon | Layout::IsoZone | Layout::IsoZoneShort => {
                if input.get(pos) == Some(&'Z') {
                    pos += 1;
                    offset = Some(0);
                    zone_name = ZoneName::Utc;
                } else {
                    let colon = token == Layout::IsoZoneColon;
                    let short = token == Layout::IsoZoneShort;
                    offset = Some(
                        parse_offset(&input, &mut pos, colon, short)
                            .ok_or_else(|| fail(pos, &token))?,
                    );
                    zone_name = ZoneName::Numeric;
                }
            }
            Layout::NumZoneColon | Layout::NumZone | Layout::NumZoneShort => {
                let colon = token == Layout::NumZoneColon;
                let short = token == Layout::NumZoneShort;
                offset = Some(
                    parse_offset(&input, &mut pos, colon, short)
                        .ok_or_else(|| fail(pos, &token))?,
                );
                zone_name = ZoneName::Numeric;
            }
        }
    }
    if pos != input.len() {
        let rest: String = input[pos..].iter().collect();
        return Err(Error(format!(
            "parsing time \"{text}\" as \"{layout}\": extra text: \"{rest}\""
        )));
    }
    if let Some(pm) = pm {
        if hour > 12 {
            return Err(Error(format!("parsing time \"{text}\": hour out of range")));
        }
        if pm && hour < 12 {
            hour += 12;
        } else if !pm && hour == 12 {
            hour = 0;
        }
    }
    if let Some(year_day) = year_day {
        let days = days_from_civil(year, 1, 1) + year_day - 1;
        let (y, m, d) = civil_from_days(days);
        year = y;
        month = m;
        day = d;
    }
    if !(1..=12).contains(&month) {
        return Err(Error(format!(
            "parsing time \"{text}\": month out of range"
        )));
    }
    if day < 1 || day > days_in_month(year, month) {
        return Err(Error(format!("parsing time \"{text}\": day out of range")));
    }
    if hour > 23 || minute > 59 || second > 59 {
        return Err(Error(format!("parsing time \"{text}\": hour out of range")));
    }
    let offset = offset.unwrap_or(0);
    let local = days_from_civil(year, month, day) * 86400 + hour * 3600 + minute * 60 + second;
    Ok(DateTime {
        unix: local - offset as i64,
        nanos,
        offset,
        zone_name,
    })
}

fn named_zone(name: &str) -> ZoneName {
    let mut bytes = [0u8; 8];
    let len = name.len().min(8);
    bytes[..len].copy_from_slice(&name.as_bytes()[..len]);
    ZoneName::Named(bytes, len)
}

fn take_fraction(input: &[char], pos: &mut usize) -> u32 {
    let start = *pos;
    while *pos < input.len() && input[*pos].is_ascii_digit() {
        *pos += 1;
    }
    let digits: String = input[start..*pos].iter().collect();
    let mut text = digits.clone();
    text.truncate(9);
    while text.len() < 9 {
        text.push('0');
    }
    text.parse().unwrap_or(0)
}

fn parse_offset(input: &[char], pos: &mut usize, colon: bool, short: bool) -> Option<i32> {
    let sign = match input.get(*pos) {
        Some('+') => 1,
        Some('-') => -1,
        _ => return None,
    };
    *pos += 1;
    let hours = take_digits(input, pos, 2, 2)?;
    if short {
        return Some(sign * (hours as i32) * 3600);
    }
    if colon {
        if input.get(*pos) != Some(&':') {
            return None;
        }
        *pos += 1;
    }
    let minutes = take_digits(input, pos, 2, 2)?;
    Some(sign * ((hours as i32) * 3600 + (minutes as i32) * 60))
}

/// `parseDateTime`: falls back to a bare date when the layout is RFC 3339.
pub fn parse_datetime(layout: &str, text: &str) -> Result<DateTime, Error> {
    match parse_with_layout(layout, text) {
        Ok(parsed) => Ok(parsed),
        Err(error) => {
            if layout == RFC3339 {
                return parse_with_layout("2006-01-02", text);
            }
            Err(error)
        }
    }
}

/// Go's `time.Format`.
pub fn format(datetime: &DateTime, layout: &str) -> String {
    let civil = datetime.civil();
    let mut out = String::new();
    for token in tokenize_layout(layout) {
        match token {
            Layout::Literal(c) => out.push(c),
            Layout::LongYear => out.push_str(&format!("{:04}", civil.year)),
            Layout::ShortYear => out.push_str(&format!("{:02}", civil.year.rem_euclid(100))),
            Layout::ZeroMonth => out.push_str(&format!("{:02}", civil.month)),
            Layout::Month => out.push_str(&civil.month.to_string()),
            Layout::ShortMonthName => out.push_str(SHORT_MONTHS[(civil.month - 1) as usize]),
            Layout::LongMonthName => out.push_str(LONG_MONTHS[(civil.month - 1) as usize]),
            Layout::ZeroDay => out.push_str(&format!("{:02}", civil.day)),
            Layout::Day => out.push_str(&civil.day.to_string()),
            Layout::UnderDay => out.push_str(&format!("{:>2}", civil.day)),
            Layout::YearDay => out.push_str(&format!("{:03}", civil.year_day)),
            Layout::ShortWeekDay => out.push_str(SHORT_DAYS[civil.weekday as usize]),
            Layout::LongWeekDay => out.push_str(LONG_DAYS[civil.weekday as usize]),
            Layout::Hour24 => out.push_str(&format!("{:02}", civil.hour)),
            Layout::ZeroHour12 => out.push_str(&format!("{:02}", twelve_hour(civil.hour))),
            Layout::Hour12 => out.push_str(&twelve_hour(civil.hour).to_string()),
            Layout::ZeroMinute => out.push_str(&format!("{:02}", civil.minute)),
            Layout::Minute => out.push_str(&civil.minute.to_string()),
            Layout::ZeroSecond => out.push_str(&format!("{:02}", civil.second)),
            Layout::Second => out.push_str(&civil.second.to_string()),
            Layout::PmUpper => out.push_str(if civil.hour >= 12 { "PM" } else { "AM" }),
            Layout::PmLower => out.push_str(if civil.hour >= 12 { "pm" } else { "am" }),
            Layout::Zone => match datetime.zone_name {
                ZoneName::Utc => out.push_str("UTC"),
                ZoneName::Named(bytes, len) => {
                    out.push_str(std::str::from_utf8(&bytes[..len]).unwrap_or(""))
                }
                ZoneName::Numeric => {
                    out.push_str(&format_offset(datetime.offset, false, false, true));
                }
            },
            Layout::IsoZoneColon => {
                if datetime.offset == 0 && datetime.zone_name == ZoneName::Utc {
                    out.push('Z');
                } else {
                    out.push_str(&format_offset(datetime.offset, true, false, false));
                }
            }
            Layout::IsoZone => {
                if datetime.offset == 0 && datetime.zone_name == ZoneName::Utc {
                    out.push('Z');
                } else {
                    out.push_str(&format_offset(datetime.offset, false, false, false));
                }
            }
            Layout::IsoZoneShort => {
                if datetime.offset == 0 && datetime.zone_name == ZoneName::Utc {
                    out.push('Z');
                } else {
                    out.push_str(&format_offset(datetime.offset, false, true, false));
                }
            }
            Layout::NumZoneColon => {
                out.push_str(&format_offset(datetime.offset, true, false, false))
            }
            Layout::NumZone => out.push_str(&format_offset(datetime.offset, false, false, false)),
            Layout::NumZoneShort => {
                out.push_str(&format_offset(datetime.offset, false, true, false))
            }
            Layout::FracZeros(count) => {
                let digits = format!("{:09}", civil.nanos);
                out.push('.');
                out.push_str(&digits[..count.min(9)]);
            }
            Layout::FracNines(count) => {
                let digits = format!("{:09}", civil.nanos);
                let trimmed = digits[..count.min(9)].trim_end_matches('0');
                if !trimmed.is_empty() {
                    out.push('.');
                    out.push_str(trimmed);
                }
            }
        }
    }
    out
}

fn twelve_hour(hour: i64) -> i64 {
    let h = hour % 12;
    if h == 0 {
        12
    } else {
        h
    }
}

fn format_offset(offset: i32, colon: bool, short: bool, zone_style: bool) -> String {
    let sign = if offset < 0 { '-' } else { '+' };
    let total = offset.abs();
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    if short {
        return format!("{sign}{hours:02}");
    }
    if zone_style && minutes == 0 {
        return format!("{sign}{hours:02}");
    }
    if colon {
        format!("{sign}{hours:02}:{minutes:02}")
    } else {
        format!("{sign}{hours:02}{minutes:02}")
    }
}

/// Go's `time.ParseDuration`, in nanoseconds.
pub fn parse_duration(text: &str) -> Result<i128, Error> {
    let original = text;
    let bad = || Error(format!("time: invalid duration \"{original}\""));
    let mut rest = text;
    let mut negative = false;
    if let Some(stripped) = rest.strip_prefix('-') {
        negative = true;
        rest = stripped;
    } else if let Some(stripped) = rest.strip_prefix('+') {
        rest = stripped;
    }
    if rest == "0" {
        return Ok(0);
    }
    if rest.is_empty() {
        return Err(bad());
    }
    let mut total: i128 = 0;
    while !rest.is_empty() {
        let bytes = rest.as_bytes();
        let mut i = 0;
        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
            i += 1;
        }
        if i == 0 {
            return Err(bad());
        }
        let number: f64 = rest[..i].parse().map_err(|_| bad())?;
        rest = &rest[i..];
        let unit_end = rest
            .find(|c: char| c.is_ascii_digit() || c == '.')
            .unwrap_or(rest.len());
        let unit = &rest[..unit_end];
        rest = &rest[unit_end..];
        let scale: f64 = match unit {
            "ns" => 1.0,
            "us" | "µs" | "μs" => 1e3,
            "ms" => 1e6,
            "s" => 1e9,
            "m" => 60e9,
            "h" => 3600e9,
            "" => {
                return Err(Error(format!(
                    "time: missing unit in duration \"{original}\""
                )))
            }
            _ => {
                return Err(Error(format!(
                    "time: unknown unit \"{unit}\" in duration \"{original}\""
                )))
            }
        };
        total += (number * scale).round() as i128;
    }
    Ok(if negative { -total } else { total })
}

/// `addDateTimes`.
pub fn add_duration(layout: &str, lhs: &str, duration: &str) -> Result<String, Error> {
    let nanos = parse_duration(duration)
        .map_err(|e| Error(format!("unable to parse duration [{duration}]: {}", e.0)))?;
    let current = parse_datetime(layout, lhs)?;
    Ok(format(&current.add_nanos(nanos), layout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_formats_rfc3339() {
        let parsed = parse_datetime(RFC3339, "2021-05-19T01:02:03Z").unwrap();
        assert_eq!(format(&parsed, RFC3339), "2021-05-19T01:02:03Z");
        let parsed = parse_datetime(RFC3339, "2021-05-19T01:02:03+02:00").unwrap();
        assert_eq!(format(&parsed, RFC3339), "2021-05-19T01:02:03+02:00");
        let parsed = parse_datetime(RFC3339, "2021-05-19").unwrap();
        assert_eq!(
            format(&parsed, "02 Jan 06 15:04 MST"),
            "19 May 21 00:00 UTC"
        );
        assert!(parse_datetime(RFC3339, "cat").is_err());
        assert_eq!(
            add_duration(RFC3339, "2021-05-19T01:02:03Z", "3h10m").unwrap(),
            "2021-05-19T04:12:03Z"
        );
        let parsed = parse_datetime(RFC3339_NANO, "2021-05-19T01:02:03.000000004Z").unwrap();
        assert_eq!(parsed.nanos, 4);
        assert_eq!(
            format(&parsed, RFC3339_NANO),
            "2021-05-19T01:02:03.000000004Z"
        );
    }
}
