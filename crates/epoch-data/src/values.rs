//! DuckDB values → JSON for the grid.
//!
//! Integers that a JavaScript number can't hold exactly, decimals, and
//! temporal values become strings, so the frontend never shows a silently
//! rounded number.

use duckdb::types::{TimeUnit, Value};
use serde_json::Value as Json;

const MAX_SAFE_INT: i128 = (1 << 53) - 1;

fn int(v: i128) -> Json {
    if (-MAX_SAFE_INT..=MAX_SAFE_INT).contains(&v) {
        Json::from(v as i64)
    } else {
        Json::String(v.to_string())
    }
}

fn float(v: f64) -> Json {
    serde_json::Number::from_f64(v)
        .map(Json::Number)
        .unwrap_or_else(|| Json::String(v.to_string()))
}

/// Days since 1970-01-01 → (year, month, day). Howard Hinnant's algorithm.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

fn date(days: i64) -> String {
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}")
}

fn time_of_day(micros: i64) -> String {
    let secs = micros.div_euclid(1_000_000);
    let frac = micros.rem_euclid(1_000_000);
    let (h, mi, s) = (secs / 3_600, (secs / 60) % 60, secs % 60);
    if frac == 0 {
        format!("{h:02}:{mi:02}:{s:02}")
    } else {
        format!("{h:02}:{mi:02}:{s:02}.{frac:06}")
    }
}

fn timestamp(unit: TimeUnit, v: i64) -> String {
    let micros = unit.to_micros(v);
    let day_micros = 86_400_000_000i64;
    format!(
        "{} {}",
        date(micros.div_euclid(day_micros)),
        time_of_day(micros.rem_euclid(day_micros))
    )
}

pub fn to_json(value: Value) -> Json {
    match value {
        Value::Null => Json::Null,
        Value::Boolean(b) => Json::Bool(b),
        Value::TinyInt(v) => Json::from(v),
        Value::SmallInt(v) => Json::from(v),
        Value::Int(v) => Json::from(v),
        Value::BigInt(v) => int(i128::from(v)),
        Value::HugeInt(v) => int(v),
        Value::UHugeInt(v) => i128::try_from(v)
            .map(int)
            .unwrap_or(Json::String(v.to_string())),
        Value::UTinyInt(v) => Json::from(v),
        Value::USmallInt(v) => Json::from(v),
        Value::UInt(v) => Json::from(v),
        Value::UBigInt(v) => int(i128::from(v)),
        Value::Float(v) => float(f64::from(v)),
        Value::Double(v) => float(v),
        Value::Decimal(d) => Json::String(d.to_string()),
        Value::Timestamp(unit, v) => Json::String(timestamp(unit, v)),
        Value::Text(s) | Value::Enum(s) => Json::String(s),
        Value::Blob(b) | Value::Geometry(b) => Json::String(format!("<{} bytes>", b.len())),
        Value::Date32(d) => Json::String(date(i64::from(d))),
        Value::Time64(unit, v) => Json::String(time_of_day(unit.to_micros(v))),
        Value::Interval {
            months,
            days,
            nanos,
        } => Json::String(format!("{months} months {days} days {}µs", nanos / 1_000)),
        Value::List(items) | Value::Array(items) => {
            Json::Array(items.into_iter().map(to_json).collect())
        }
        Value::Struct(fields) => Json::Object(
            fields
                .iter()
                .map(|(k, v)| (k.clone(), to_json(v.clone())))
                .collect(),
        ),
        Value::Map(entries) => Json::Array(
            entries
                .iter()
                .map(|(k, v)| Json::Array(vec![to_json(k.clone()), to_json(v.clone())]))
                .collect(),
        ),
        Value::Union(inner) => to_json(*inner),
        // `Value` is non-exhaustive; show anything newer as DuckDB debug text.
        other => Json::String(format!("{other:?}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_times_format_like_duckdb() {
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(19_723), "2024-01-01");
        assert_eq!(date(-1), "1969-12-31");
        assert_eq!(
            timestamp(TimeUnit::Second, 1_700_000_000),
            "2023-11-14 22:13:20"
        );
        assert_eq!(time_of_day(1_500_000), "00:00:01.500000");
    }

    #[test]
    fn large_integers_become_strings() {
        assert_eq!(to_json(Value::BigInt(42)), Json::from(42));
        assert_eq!(
            to_json(Value::BigInt(i64::MAX)),
            Json::String(i64::MAX.to_string())
        );
        assert_eq!(to_json(Value::Double(f64::NAN)), Json::String("NaN".into()));
    }
}
