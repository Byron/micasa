// Copyright 2026 Phillip Cloud
// Licensed under the Apache License, Version 2.0

use std::num::IntErrorKind;
use time::macros::format_description;
use time::{Date, Month};

pub const DATE_LAYOUT: &str = "YYYY-MM-DD";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationError {
    InvalidMoney,
    NegativeMoney,
    InvalidDate,
    InvalidInt,
    InvalidFloat,
    InvalidInterval,
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMoney => f.write_str("invalid money value"),
            Self::NegativeMoney => f.write_str("negative money value"),
            Self::InvalidDate => f.write_str("invalid date value"),
            Self::InvalidInt => f.write_str("invalid integer value"),
            Self::InvalidFloat => f.write_str("invalid decimal value"),
            Self::InvalidInterval => f.write_str("invalid interval value"),
        }
    }
}

impl std::error::Error for ValidationError {}

pub type ValidationResult<T> = std::result::Result<T, ValidationError>;

pub fn parse_required_cents(input: &str) -> ValidationResult<i64> {
    parse_cents(input.trim())
}

pub fn parse_optional_cents(input: &str) -> ValidationResult<Option<i64>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    parse_cents(trimmed).map(Some)
}

pub fn format_cents(cents: i64) -> String {
    let (sign, cents) = normalize_sign(cents);
    let dollars = cents / 100;
    let remainder = cents % 100;
    format!("{sign}${}.{:02}", comma_format(dollars), remainder)
}

pub fn format_optional_cents(cents: Option<i64>) -> String {
    cents.map_or_else(String::new, format_cents)
}

pub fn format_compact_cents(cents: i64) -> String {
    let (sign, cents) = normalize_sign(cents);
    let dollars = (cents as f64) / 100.0;
    if dollars < 1000.0 {
        return format!("{sign}{}", format_cents(cents));
    }

    let (value, suffix) = if dollars < 1_000_000.0 {
        (dollars / 1000.0, "k")
    } else if dollars < 1_000_000_000.0 {
        (dollars / 1_000_000.0, "M")
    } else {
        (dollars / 1_000_000_000.0, "B")
    };

    let rounded = (value * 10.0).round() / 10.0;
    if rounded.fract().abs() < f64::EPSILON {
        format!("{sign}${:.0}{suffix}", rounded)
    } else {
        format!("{sign}${rounded:.1}{suffix}")
    }
}

pub fn format_compact_optional_cents(cents: Option<i64>) -> String {
    cents.map_or_else(String::new, format_compact_cents)
}

pub fn parse_required_date(input: &str) -> ValidationResult<Date> {
    parse_date(input.trim())
}

pub fn parse_optional_date(input: &str) -> ValidationResult<Option<Date>> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    parse_date(trimmed).map(Some)
}

pub fn format_date(value: Option<Date>) -> String {
    let Some(value) = value else {
        return String::new();
    };
    value
        .format(&format_description!("[year]-[month]-[day]"))
        .expect("date format is valid")
}

pub fn parse_optional_int(input: &str) -> ValidationResult<i32> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(0);
    }
    let value = trimmed
        .parse::<i32>()
        .map_err(|_| ValidationError::InvalidInt)?;
    if value < 0 {
        return Err(ValidationError::InvalidInt);
    }
    Ok(value)
}

pub fn parse_required_int(input: &str) -> ValidationResult<i32> {
    if input.trim().is_empty() {
        return Err(ValidationError::InvalidInt);
    }
    parse_optional_int(input)
}

pub fn parse_optional_float(input: &str) -> ValidationResult<f64> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(0.0);
    }
    let value = trimmed
        .parse::<f64>()
        .map_err(|_| ValidationError::InvalidFloat)?;
    if value < 0.0 {
        return Err(ValidationError::InvalidFloat);
    }
    Ok(value)
}

pub fn parse_required_float(input: &str) -> ValidationResult<f64> {
    if input.trim().is_empty() {
        return Err(ValidationError::InvalidFloat);
    }
    parse_optional_float(input)
}

pub fn parse_interval_months(input: &str) -> ValidationResult<i32> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(0);
    }

    match trimmed.parse::<i32>() {
        Ok(value) => {
            if value < 0 {
                return Err(ValidationError::InvalidInterval);
            }
            return Ok(value);
        }
        Err(error) => {
            if error.kind() != &IntErrorKind::InvalidDigit {
                return Err(ValidationError::InvalidInterval);
            }
        }
    }

    let bytes = trimmed.as_bytes();
    let mut index = 0usize;
    let mut total = 0i32;
    let mut parsed_any = false;

    skip_ascii_whitespace(bytes, &mut index);
    if let Some(years) = parse_unit(bytes, &mut index, b'y', true)? {
        total = total
            .checked_add(
                years
                    .checked_mul(12)
                    .ok_or(ValidationError::InvalidInterval)?,
            )
            .ok_or(ValidationError::InvalidInterval)?;
        parsed_any = true;
    }

    skip_ascii_whitespace(bytes, &mut index);
    if let Some(months) = parse_unit(bytes, &mut index, b'm', false)? {
        total = total
            .checked_add(months)
            .ok_or(ValidationError::InvalidInterval)?;
        parsed_any = true;
    }

    skip_ascii_whitespace(bytes, &mut index);
    if index != bytes.len() || !parsed_any {
        return Err(ValidationError::InvalidInterval);
    }
    Ok(total)
}

pub fn compute_next_due(last: Option<Date>, interval_months: i32) -> Option<Date> {
    let last = last?;
    if interval_months <= 0 {
        return None;
    }
    Some(add_months(last, interval_months))
}

pub fn add_months(date: Date, months: i32) -> Date {
    let base_month = i32::from(date.month() as u8);
    let total_month = base_month - 1 + months;
    let year = date.year() + total_month.div_euclid(12);
    let month_number = (total_month.rem_euclid(12) + 1) as u8;
    let month = Month::try_from(month_number).expect("month value from modulo is valid");
    let day = date.day().min(last_day_of_month(year, month));
    Date::from_calendar_date(year, month, day).expect("derived date is valid")
}

fn parse_cents(input: &str) -> ValidationResult<i64> {
    let clean = input.replace(',', "");
    if clean.starts_with('-') {
        return Err(ValidationError::NegativeMoney);
    }

    let clean = clean.strip_prefix('$').unwrap_or(&clean);
    if clean.is_empty() {
        return Err(ValidationError::InvalidMoney);
    }

    let parts = clean.split('.').collect::<Vec<_>>();
    if parts.len() > 2 {
        return Err(ValidationError::InvalidMoney);
    }

    let whole = parse_digits(parts[0], true)?;
    if whole > i64::MAX / 100 {
        return Err(ValidationError::InvalidMoney);
    }

    let mut frac = 0i64;
    if parts.len() == 2 {
        if parts[1].len() > 2 {
            return Err(ValidationError::InvalidMoney);
        }
        frac = parse_digits(parts[1], false)?;
        if parts[1].len() == 1 {
            frac = frac.checked_mul(10).ok_or(ValidationError::InvalidMoney)?;
        }
    }

    whole
        .checked_mul(100)
        .and_then(|value| value.checked_add(frac))
        .ok_or(ValidationError::InvalidMoney)
}

fn parse_digits(input: &str, allow_empty: bool) -> ValidationResult<i64> {
    if input.is_empty() {
        if allow_empty {
            return Ok(0);
        }
        return Err(ValidationError::InvalidMoney);
    }
    if !input.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(ValidationError::InvalidMoney);
    }
    input
        .parse::<i64>()
        .map_err(|_| ValidationError::InvalidMoney)
}

fn parse_date(input: &str) -> ValidationResult<Date> {
    Date::parse(input, &format_description!("[year]-[month]-[day]"))
        .map_err(|_| ValidationError::InvalidDate)
}

fn comma_format(value: i64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    let mut chars = digits.chars().collect::<Vec<_>>();
    let mut count = 0usize;
    while let Some(ch) = chars.pop() {
        if count == 3 {
            out.push(',');
            count = 0;
        }
        out.push(ch);
        count += 1;
    }
    out.chars().rev().collect()
}

fn normalize_sign(cents: i64) -> (&'static str, i64) {
    if cents >= 0 {
        return ("", cents);
    }
    if cents == i64::MIN {
        ("-", i64::MAX)
    } else {
        ("-", -cents)
    }
}

fn parse_unit(
    bytes: &[u8],
    index: &mut usize,
    suffix: u8,
    rollback_on_mismatch: bool,
) -> ValidationResult<Option<i32>> {
    let start = *index;
    while *index < bytes.len() && bytes[*index].is_ascii_digit() {
        *index += 1;
    }
    if *index == start {
        return Ok(None);
    }
    let digits_end = *index;

    skip_ascii_whitespace(bytes, index);
    if *index >= bytes.len() || !bytes[*index].eq_ignore_ascii_case(&suffix) {
        if rollback_on_mismatch {
            *index = start;
            return Ok(None);
        }
        return Err(ValidationError::InvalidInterval);
    }
    *index += 1;

    let number = std::str::from_utf8(&bytes[start..digits_end]).expect("substring is valid utf-8");
    let parsed = number
        .parse::<i32>()
        .map_err(|_| ValidationError::InvalidInterval)?;
    Ok(Some(parsed))
}

fn skip_ascii_whitespace(bytes: &[u8], index: &mut usize) {
    while *index < bytes.len() && bytes[*index].is_ascii_whitespace() {
        *index += 1;
    }
}

fn last_day_of_month(year: i32, month: Month) -> u8 {
    let (next_year, next_month) = if month == Month::December {
        (year + 1, Month::January)
    } else {
        (
            year,
            Month::try_from(month as u8 + 1).expect("next month exists"),
        )
    };
    let first_next = Date::from_calendar_date(next_year, next_month, 1).expect("valid date");
    let last = first_next.previous_day().expect("previous day exists");
    last.day()
}

#[cfg(test)]
mod tests {
    use super::{
        ValidationError, add_months, compute_next_due, format_cents, format_compact_cents,
        format_compact_optional_cents, format_date, format_optional_cents, parse_interval_months,
        parse_optional_cents, parse_optional_date, parse_optional_float, parse_optional_int,
        parse_required_cents, parse_required_date, parse_required_float, parse_required_int,
    };
    use std::collections::BTreeMap;
    use time::{Date, Month};

    #[test]
    fn parse_required_cents_test() {
        let cases = BTreeMap::from([
            ("100", 10_000),
            ("100.5", 10_050),
            ("100.05", 10_005),
            ("$1,234.56", 123_456),
            (".75", 75),
            ("0.99", 99),
        ]);
        for (input, expected) in cases {
            let got = parse_required_cents(input).expect("money should parse");
            assert_eq!(got, expected, "input {input}");
        }
    }

    #[test]
    fn parse_required_cents_invalid() {
        for input in ["", "12.345", "abc", "1.2.3"] {
            assert!(parse_required_cents(input).is_err(), "input {input}");
        }
    }

    #[test]
    fn parse_optional_cents_test() {
        let empty = parse_optional_cents("").expect("empty is valid");
        assert!(empty.is_none());

        let value = parse_optional_cents("5").expect("value should parse");
        assert_eq!(value, Some(500));
    }

    #[test]
    fn format_cents_test() {
        assert_eq!(format_cents(123_456), "$1,234.56");
    }

    #[test]
    fn parse_optional_date_test() {
        let parsed = parse_optional_date("2025-06-11")
            .expect("date should parse")
            .expect("date should be present");
        assert_eq!(parsed.to_string(), "2025-06-11");

        assert!(parse_optional_date("06/11/2025").is_err());
    }

    #[test]
    fn parse_optional_int_test() {
        let value = parse_optional_int("12").expect("int should parse");
        assert_eq!(value, 12);
        assert!(parse_optional_int("-1").is_err());
    }

    #[test]
    fn parse_optional_float_test() {
        let value = parse_optional_float("2.5").expect("float should parse");
        assert_eq!(value, 2.5);
        assert!(parse_optional_float("-1.2").is_err());
    }

    #[test]
    fn format_optional_cents_test() {
        assert_eq!(format_optional_cents(None), "");
        assert_eq!(format_optional_cents(Some(123_456)), "$1,234.56");
    }

    #[test]
    fn format_cents_negative() {
        assert_eq!(format_cents(-500), "-$5.00");
    }

    #[test]
    fn parse_cents_rejects_negative() {
        for input in ["-$5.00", "-5.00", "-$1,234.56"] {
            let err = parse_required_cents(input).expect_err("negative should fail");
            assert_eq!(err, ValidationError::NegativeMoney);
        }
        for input in ["$-100", "--$5", "-", "-$"] {
            assert!(parse_required_cents(input).is_err(), "input {input}");
        }
    }

    #[test]
    fn parse_cents_format_roundtrip() {
        for cents in [0_i64, 1, 99, 100, 123_456] {
            let formatted = format_cents(cents);
            let parsed = parse_required_cents(&formatted).expect("formatted cents should parse");
            assert_eq!(parsed, cents, "formatted={formatted}");
        }
    }

    #[test]
    fn format_cents_zero() {
        assert_eq!(format_cents(0), "$0.00");
    }

    #[test]
    fn parse_required_date_test() {
        let cases = [("2025-06-11", "2025-06-11"), (" 2025-06-11 ", "2025-06-11")];
        for (input, expected) in cases {
            let got = parse_required_date(input).expect("date should parse");
            assert_eq!(got.to_string(), expected, "input={input}");
        }
    }

    #[test]
    fn parse_required_date_invalid() {
        for input in ["", "06/11/2025", "not-a-date", "2025-13-01"] {
            assert!(parse_required_date(input).is_err(), "input {input}");
        }
    }

    #[test]
    fn format_date_test() {
        assert_eq!(format_date(None), "");
        let value = Date::from_calendar_date(2025, Month::June, 11).expect("valid date");
        assert_eq!(format_date(Some(value)), "2025-06-11");
    }

    #[test]
    fn parse_required_int_test() {
        let cases = [("42", 42), (" 7 ", 7), ("0", 0)];
        for (input, expected) in cases {
            let got = parse_required_int(input).expect("int should parse");
            assert_eq!(got, expected, "input {input}");
        }
    }

    #[test]
    fn parse_required_int_invalid() {
        for input in ["", "abc", "-5", "1.5"] {
            assert!(parse_required_int(input).is_err(), "input {input}");
        }
    }

    #[test]
    fn parse_required_float_test() {
        let cases = [("2.5", 2.5), (" 0 ", 0.0), ("100", 100.0)];
        for (input, expected) in cases {
            let got = parse_required_float(input).expect("float should parse");
            assert_eq!(got, expected, "input {input}");
        }
    }

    #[test]
    fn parse_required_float_invalid() {
        for input in ["", "abc", "-1.5"] {
            assert!(parse_required_float(input).is_err(), "input {input}");
        }
    }

    #[test]
    fn parse_optional_int_empty() {
        assert_eq!(parse_optional_int("").expect("empty optional int"), 0);
    }

    #[test]
    fn parse_optional_float_empty() {
        assert_eq!(parse_optional_float("").expect("empty optional float"), 0.0);
    }

    #[test]
    fn parse_optional_date_empty() {
        assert_eq!(parse_optional_date("").expect("empty optional date"), None);
    }

    #[test]
    fn parse_optional_cents_invalid() {
        assert!(parse_optional_cents("abc").is_err());
    }

    #[test]
    fn compute_next_due_test() {
        let last = Date::from_calendar_date(2024, Month::October, 10).expect("valid date");
        let next = compute_next_due(Some(last), 6).expect("next due should exist");
        assert_eq!(next.to_string(), "2025-04-10");
    }

    #[test]
    fn compute_next_due_nil_date() {
        assert_eq!(compute_next_due(None, 6), None);
    }

    #[test]
    fn compute_next_due_zero_interval() {
        let date = Date::from_calendar_date(2024, Month::January, 1).expect("valid date");
        assert_eq!(compute_next_due(Some(date), 0), None);
    }

    #[test]
    fn format_compact_cents_test() {
        let cases = [
            (0, "$0.00"),
            (999, "$9.99"),
            (10_000, "$100.00"),
            (99_999, "$999.99"),
            (100_000, "$1k"),
            (123_456, "$1.2k"),
            (4_500_000, "$45k"),
            (5_234_023, "$52.3k"),
            (100_000_000, "$1M"),
            (130_000_000, "$1.3M"),
            (200_000_000, "$2M"),
            (-500, "-$5.00"),
            (-250_000, "-$2.5k"),
            (-100_000_000, "-$1M"),
        ];
        for (input, expected) in cases {
            assert_eq!(format_compact_cents(input), expected, "input={input}");
        }
    }

    #[test]
    fn format_compact_optional_cents_test() {
        assert_eq!(format_compact_optional_cents(None), "");
        assert_eq!(format_compact_optional_cents(Some(250_000)), "$2.5k");
    }

    #[test]
    fn parse_cents_overflow() {
        for input in [
            "$92233720368547759.00",
            "$999999999999999999999.99",
            "$92233720368547758.08",
            "$92233720368547758.99",
        ] {
            assert!(parse_required_cents(input).is_err(), "input {input}");
        }
    }

    #[test]
    fn parse_cents_at_max_safe_value() {
        let max_no_frac =
            parse_required_cents("$92233720368547758.00").expect("boundary value should parse");
        assert_eq!(max_no_frac, 9_223_372_036_854_775_800);

        let max_with_frac =
            parse_required_cents("$92233720368547758.07").expect("fraction boundary should parse");
        assert_eq!(max_with_frac, i64::MAX);
    }

    #[test]
    fn format_cents_min_int64() {
        let formatted = format_cents(i64::MIN);
        assert!(formatted.contains("-$"));
        assert!(formatted.contains("92,233,720,368,547,758.07"));
    }

    #[test]
    fn format_compact_cents_min_int64() {
        let formatted = format_compact_cents(i64::MIN);
        assert!(formatted.contains("-$"));
    }

    #[test]
    fn add_months_test() {
        let cases = [
            ((2025, Month::January, 31, 1), "2025-02-28"),
            ((2024, Month::January, 31, 1), "2024-02-29"),
            ((2025, Month::March, 31, 1), "2025-04-30"),
            ((2025, Month::January, 15, 1), "2025-02-15"),
            ((2025, Month::January, 31, 3), "2025-04-30"),
            ((2024, Month::November, 30, 3), "2025-02-28"),
            ((2024, Month::February, 29, 12), "2025-02-28"),
        ];
        for ((year, month, day, months), expected) in cases {
            let start = Date::from_calendar_date(year, month, day).expect("valid test date");
            let got = add_months(start, months);
            assert_eq!(got.to_string(), expected);
        }
    }

    #[test]
    fn parse_interval_months_test() {
        let cases = [
            ("12", 12),
            ("0", 0),
            ("  7  ", 7),
            ("6m", 6),
            ("6M", 6),
            (" 3m ", 3),
            ("1y", 12),
            ("2Y", 24),
            (" 1y ", 12),
            ("2y 6m", 30),
            ("1y6m", 18),
            ("1Y 3M", 15),
            ("  2y  6m  ", 30),
            ("", 0),
            ("   ", 0),
        ];
        for (input, expected) in cases {
            let got = parse_interval_months(input).expect("interval should parse");
            assert_eq!(got, expected, "input={input}");
        }
    }

    #[test]
    fn parse_interval_months_invalid() {
        for input in ["abc", "-1", "1.5m", "1x", "m", "y", "6m 1y"] {
            assert!(parse_interval_months(input).is_err(), "input={input}");
        }
    }

    #[test]
    fn compute_next_due_month_end_clamping() {
        let last = Date::from_calendar_date(2025, Month::January, 31).expect("valid date");
        let next = compute_next_due(Some(last), 1).expect("next due should exist");
        assert_eq!(next.to_string(), "2025-02-28");
    }
}
