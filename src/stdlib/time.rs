use crate::interpreter::Value;
use chrono::{
    DateTime, Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Timelike,
    Utc,
};
use chrono_tz::Tz;
use indexmap::IndexMap;

pub fn create_module() -> Value {
    let mut m = IndexMap::new();
    m.insert("now".to_string(), Value::BuiltIn("time.now".to_string()));
    m.insert("unix".to_string(), Value::BuiltIn("time.unix".to_string()));
    m.insert(
        "parse".to_string(),
        Value::BuiltIn("time.parse".to_string()),
    );
    m.insert(
        "format".to_string(),
        Value::BuiltIn("time.format".to_string()),
    );
    m.insert("diff".to_string(), Value::BuiltIn("time.diff".to_string()));
    m.insert("add".to_string(), Value::BuiltIn("time.add".to_string()));
    m.insert("sub".to_string(), Value::BuiltIn("time.sub".to_string()));
    m.insert("zone".to_string(), Value::BuiltIn("time.zone".to_string()));
    m.insert(
        "zones".to_string(),
        Value::BuiltIn("time.zones".to_string()),
    );
    m.insert(
        "elapsed".to_string(),
        Value::BuiltIn("time.elapsed".to_string()),
    );
    m.insert(
        "is_before".to_string(),
        Value::BuiltIn("time.is_before".to_string()),
    );
    m.insert(
        "is_after".to_string(),
        Value::BuiltIn("time.is_after".to_string()),
    );
    m.insert(
        "start_of".to_string(),
        Value::BuiltIn("time.start_of".to_string()),
    );
    m.insert(
        "end_of".to_string(),
        Value::BuiltIn("time.end_of".to_string()),
    );
    m.insert(
        "from_unix".to_string(),
        Value::BuiltIn("time.from_unix".to_string()),
    );
    m.insert(
        "today".to_string(),
        Value::BuiltIn("time.today".to_string()),
    );
    m.insert("date".to_string(), Value::BuiltIn("time.date".to_string()));
    m.insert(
        "sleep".to_string(),
        Value::BuiltIn("time.sleep".to_string()),
    );
    m.insert(
        "measure".to_string(),
        Value::BuiltIn("time.measure".to_string()),
    );
    m.insert(
        "local".to_string(),
        Value::BuiltIn("time.local".to_string()),
    );
    m.insert(
        "is_weekend".to_string(),
        Value::BuiltIn("time.is_weekend".to_string()),
    );
    m.insert(
        "is_weekday".to_string(),
        Value::BuiltIn("time.is_weekday".to_string()),
    );
    m.insert(
        "day_of_week".to_string(),
        Value::BuiltIn("time.day_of_week".to_string()),
    );
    m.insert(
        "days_in_month".to_string(),
        Value::BuiltIn("time.days_in_month".to_string()),
    );
    m.insert(
        "is_leap_year".to_string(),
        Value::BuiltIn("time.is_leap_year".to_string()),
    );
    Value::Object(m)
}

fn datetime_to_value(dt: DateTime<Utc>, tz_name: &str) -> Value {
    let mut m = IndexMap::new();
    m.insert("iso".to_string(), Value::String(dt.to_rfc3339()));
    m.insert("unix".to_string(), Value::Int(dt.timestamp()));
    m.insert("unix_ms".to_string(), Value::Int(dt.timestamp_millis()));
    m.insert("year".to_string(), Value::Int(dt.year() as i64));
    m.insert("month".to_string(), Value::Int(dt.month() as i64));
    m.insert("day".to_string(), Value::Int(dt.day() as i64));
    m.insert("hour".to_string(), Value::Int(dt.hour() as i64));
    m.insert("minute".to_string(), Value::Int(dt.minute() as i64));
    m.insert("second".to_string(), Value::Int(dt.second() as i64));
    m.insert(
        "weekday".to_string(),
        Value::String(dt.format("%A").to_string()),
    );
    m.insert(
        "weekday_short".to_string(),
        Value::String(dt.format("%a").to_string()),
    );
    m.insert("day_of_year".to_string(), Value::Int(dt.ordinal() as i64));
    m.insert("timezone".to_string(), Value::String(tz_name.to_string()));
    Value::Object(m)
}

fn datetime_tz_to_value<T: TimeZone>(dt: DateTime<T>, tz_name: &str) -> Value
where
    T::Offset: std::fmt::Display,
{
    let mut m = IndexMap::new();
    m.insert("iso".to_string(), Value::String(dt.to_rfc3339()));
    m.insert("unix".to_string(), Value::Int(dt.timestamp()));
    m.insert("unix_ms".to_string(), Value::Int(dt.timestamp_millis()));
    m.insert("year".to_string(), Value::Int(dt.year() as i64));
    m.insert("month".to_string(), Value::Int(dt.month() as i64));
    m.insert("day".to_string(), Value::Int(dt.day() as i64));
    m.insert("hour".to_string(), Value::Int(dt.hour() as i64));
    m.insert("minute".to_string(), Value::Int(dt.minute() as i64));
    m.insert("second".to_string(), Value::Int(dt.second() as i64));
    m.insert(
        "weekday".to_string(),
        Value::String(dt.format("%A").to_string()),
    );
    m.insert(
        "weekday_short".to_string(),
        Value::String(dt.format("%a").to_string()),
    );
    m.insert("day_of_year".to_string(), Value::Int(dt.ordinal() as i64));
    m.insert("timezone".to_string(), Value::String(tz_name.to_string()));
    Value::Object(m)
}

fn extract_unix(val: &Value) -> Option<i64> {
    match val {
        Value::Object(m) => m.get("unix").and_then(|v| match v {
            Value::Int(n) => Some(*n),
            _ => None,
        }),
        Value::Int(n) => Some(*n),
        _ => None,
    }
}

fn parse_tz(name: &str) -> Result<Tz, String> {
    name.parse::<Tz>().map_err(|_| {
        format!(
            "unknown timezone: '{}'. Use time.zones() to list available zones",
            name
        )
    })
}

fn apply_duration(dt: DateTime<Utc>, obj: &IndexMap<String, Value>, add: bool) -> DateTime<Utc> {
    let sign = if add { 1 } else { -1 };
    let mut result = dt;
    if let Some(Value::Int(n)) = obj.get("years") {
        result = result + Duration::days(sign * n * 365);
    }
    if let Some(Value::Int(n)) = obj.get("months") {
        result = result + Duration::days(sign * n * 30);
    }
    if let Some(Value::Int(n)) = obj.get("weeks") {
        result = result + Duration::weeks(sign * n);
    }
    if let Some(Value::Int(n)) = obj.get("days") {
        result = result + Duration::days(sign * n);
    }
    if let Some(Value::Int(n)) = obj.get("hours") {
        result = result + Duration::hours(sign * n);
    }
    if let Some(Value::Int(n)) = obj.get("minutes") {
        result = result + Duration::minutes(sign * n);
    }
    if let Some(Value::Int(n)) = obj.get("seconds") {
        result = result + Duration::seconds(sign * n);
    }
    if let Some(Value::Int(n)) = obj.get("millis") {
        result = result + Duration::milliseconds(sign * n);
    }
    result
}

fn parse_datetime(s: &str) -> Result<DateTime<Utc>, String> {
    // Try RFC 3339 / ISO 8601 with timezone
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }
    // Try ISO 8601 without timezone
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S") {
        return Ok(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));
    }
    // Try date + time
    if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
        return Ok(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));
    }
    // Try date only
    if let Ok(d) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        let dt = d.and_hms_opt(0, 0, 0).unwrap();
        return Ok(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));
    }
    // Try US format: MM/DD/YYYY
    if let Ok(d) = NaiveDate::parse_from_str(s, "%m/%d/%Y") {
        let dt = d.and_hms_opt(0, 0, 0).unwrap();
        return Ok(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));
    }
    // Try European format: DD.MM.YYYY
    if let Ok(d) = NaiveDate::parse_from_str(s, "%d.%m.%Y") {
        let dt = d.and_hms_opt(0, 0, 0).unwrap();
        return Ok(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));
    }
    // Try month-first: "Jan 15, 2026"
    if let Ok(d) = NaiveDate::parse_from_str(s, "%b %d, %Y") {
        let dt = d.and_hms_opt(0, 0, 0).unwrap();
        return Ok(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));
    }
    // Try "15 Jan 2026"
    if let Ok(d) = NaiveDate::parse_from_str(s, "%d %b %Y") {
        let dt = d.and_hms_opt(0, 0, 0).unwrap();
        return Ok(DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc));
    }
    Err(format!(
        "cannot parse '{}' as a date/time. Supported formats: YYYY-MM-DD, YYYY-MM-DDTHH:MM:SS, MM/DD/YYYY, DD.MM.YYYY, \"Jan 15, 2026\"",
        s
    ))
}

pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "time.now" => {
            let now = Utc::now();
            match args.first() {
                Some(Value::String(tz_name)) => {
                    let tz = parse_tz(tz_name)?;
                    Ok(datetime_tz_to_value(now.with_timezone(&tz), tz_name))
                }
                _ => Ok(datetime_to_value(now, "UTC")),
            }
        }

        "time.local" => {
            let now = Local::now();
            Ok(datetime_tz_to_value(now, "Local"))
        }

        "time.unix" => Ok(Value::Int(Utc::now().timestamp())),

        "time.today" => {
            let now = Utc::now();
            Ok(Value::String(now.format("%Y-%m-%d").to_string()))
        }

        "time.date" => match (&args.first(), args.get(1), args.get(2)) {
            (Some(Value::Int(y)), Some(Value::Int(m)), Some(Value::Int(d))) => {
                let date = NaiveDate::from_ymd_opt(*y as i32, *m as u32, *d as u32)
                    .ok_or_else(|| format!("invalid date: {}-{}-{}", y, m, d))?;
                let dt = date.and_hms_opt(0, 0, 0).unwrap();
                let utc = DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc);
                Ok(datetime_to_value(utc, "UTC"))
            }
            _ => Err("time.date(year, month, day) requires three integers".to_string()),
        },

        "time.parse" => match args.first() {
            Some(Value::String(s)) => match args.get(1) {
                Some(Value::String(tz_name)) => {
                    let dt = parse_datetime(s)?;
                    let tz = parse_tz(tz_name)?;
                    Ok(datetime_tz_to_value(dt.with_timezone(&tz), tz_name))
                }
                _ => {
                    let dt = parse_datetime(s)?;
                    Ok(datetime_to_value(dt, "UTC"))
                }
            },
            Some(Value::Int(unix)) => {
                let dt = DateTime::from_timestamp(*unix, 0)
                    .ok_or_else(|| format!("invalid unix timestamp: {}", unix))?;
                Ok(datetime_to_value(dt, "UTC"))
            }
            _ => Err("time.parse() requires a date string or unix timestamp".to_string()),
        },

        "time.format" => {
            let t = args
                .first()
                .ok_or("time.format(time_obj, format_str) requires a time object")?;
            let fmt = match args.get(1) {
                Some(Value::String(f)) => f.as_str(),
                _ => "%Y-%m-%d %H:%M:%S",
            };
            let unix = extract_unix(t)
                .ok_or("time.format() first argument must be a time object or unix timestamp")?;
            let dt = DateTime::from_timestamp(unix, 0).ok_or("invalid timestamp")?;
            Ok(Value::String(dt.format(fmt).to_string()))
        }

        "time.from_unix" => match args.first() {
            Some(Value::Int(ts)) => {
                let dt = DateTime::from_timestamp(*ts, 0)
                    .ok_or_else(|| format!("invalid unix timestamp: {}", ts))?;
                Ok(datetime_to_value(dt, "UTC"))
            }
            _ => Err("time.from_unix() requires an integer timestamp".to_string()),
        },

        "time.diff" => {
            let a = args
                .first()
                .ok_or("time.diff(t1, t2) requires two time objects")?;
            let b = args
                .get(1)
                .ok_or("time.diff(t1, t2) requires two time objects")?;
            let unix_a = extract_unix(a).ok_or("first argument must be a time object")?;
            let unix_b = extract_unix(b).ok_or("second argument must be a time object")?;
            let diff_secs = unix_a - unix_b;
            let abs_diff = diff_secs.unsigned_abs();

            let mut result = IndexMap::new();
            result.insert("seconds".to_string(), Value::Int(diff_secs));
            result.insert("minutes".to_string(), Value::Float(diff_secs as f64 / 60.0));
            result.insert("hours".to_string(), Value::Float(diff_secs as f64 / 3600.0));
            result.insert("days".to_string(), Value::Float(diff_secs as f64 / 86400.0));
            result.insert(
                "weeks".to_string(),
                Value::Float(diff_secs as f64 / 604800.0),
            );

            let d = abs_diff / 86400;
            let h = (abs_diff % 86400) / 3600;
            let m = (abs_diff % 3600) / 60;
            let s = abs_diff % 60;
            let sign = if diff_secs < 0 { "-" } else { "" };
            result.insert(
                "human".to_string(),
                Value::String(format!("{}{}d {}h {}m {}s", sign, d, h, m, s)),
            );
            Ok(Value::Object(result))
        }

        "time.add" => {
            let t = args
                .first()
                .ok_or("time.add(time_obj, duration) requires two arguments")?;
            let unix = extract_unix(t).ok_or("first argument must be a time object")?;
            let dt = DateTime::from_timestamp(unix, 0).ok_or("invalid timestamp")?;
            match args.get(1) {
                Some(Value::Object(dur)) => {
                    let new_dt = apply_duration(dt, dur, true);
                    let tz = match t {
                        Value::Object(m) => m.get("timezone")
                            .and_then(|v| if let Value::String(s) = v { Some(s.as_str()) } else { None })
                            .unwrap_or("UTC"),
                        _ => "UTC",
                    };
                    Ok(datetime_to_value(new_dt, tz))
                }
                Some(Value::Int(secs)) => {
                    let new_dt = dt + Duration::seconds(*secs);
                    Ok(datetime_to_value(new_dt, "UTC"))
                }
                _ => Err("time.add() second argument must be a duration object like {days: 5, hours: 3} or seconds integer".to_string()),
            }
        }

        "time.sub" => {
            let t = args
                .first()
                .ok_or("time.sub(time_obj, duration) requires two arguments")?;
            let unix = extract_unix(t).ok_or("first argument must be a time object")?;
            let dt = DateTime::from_timestamp(unix, 0).ok_or("invalid timestamp")?;
            match args.get(1) {
                Some(Value::Object(dur)) => {
                    let new_dt = apply_duration(dt, dur, false);
                    let tz = match t {
                        Value::Object(m) => m
                            .get("timezone")
                            .and_then(|v| {
                                if let Value::String(s) = v {
                                    Some(s.as_str())
                                } else {
                                    None
                                }
                            })
                            .unwrap_or("UTC"),
                        _ => "UTC",
                    };
                    Ok(datetime_to_value(new_dt, tz))
                }
                Some(Value::Int(secs)) => {
                    let new_dt = dt - Duration::seconds(*secs);
                    Ok(datetime_to_value(new_dt, "UTC"))
                }
                _ => Err(
                    "time.sub() second argument must be a duration object or seconds integer"
                        .to_string(),
                ),
            }
        }

        "time.zone" => {
            let t = args
                .first()
                .ok_or("time.zone(time_obj, timezone) requires two arguments")?;
            let tz_name = match args.get(1) {
                Some(Value::String(s)) => s.as_str(),
                _ => {
                    return Err("time.zone() second argument must be a timezone string".to_string())
                }
            };
            let unix = extract_unix(t).ok_or("first argument must be a time object")?;
            let dt = DateTime::from_timestamp(unix, 0).ok_or("invalid timestamp")?;
            let tz = parse_tz(tz_name)?;
            Ok(datetime_tz_to_value(dt.with_timezone(&tz), tz_name))
        }

        "time.zones" => {
            let filter = match args.first() {
                Some(Value::String(s)) => s.to_lowercase(),
                _ => String::new(),
            };
            let zones: Vec<Value> = chrono_tz::TZ_VARIANTS
                .iter()
                .map(|tz| tz.name().to_string())
                .filter(|name| filter.is_empty() || name.to_lowercase().contains(&filter))
                .map(Value::String)
                .collect();
            Ok(Value::Array(zones))
        }

        "time.is_before" => {
            let a = args
                .first()
                .ok_or("time.is_before(t1, t2) requires two time objects")?;
            let b = args
                .get(1)
                .ok_or("time.is_before(t1, t2) requires two time objects")?;
            let unix_a = extract_unix(a).ok_or("first argument must be a time object")?;
            let unix_b = extract_unix(b).ok_or("second argument must be a time object")?;
            Ok(Value::Bool(unix_a < unix_b))
        }

        "time.is_after" => {
            let a = args
                .first()
                .ok_or("time.is_after(t1, t2) requires two time objects")?;
            let b = args
                .get(1)
                .ok_or("time.is_after(t1, t2) requires two time objects")?;
            let unix_a = extract_unix(a).ok_or("first argument must be a time object")?;
            let unix_b = extract_unix(b).ok_or("second argument must be a time object")?;
            Ok(Value::Bool(unix_a > unix_b))
        }

        "time.start_of" => {
            let t = args
                .first()
                .ok_or("time.start_of(time_obj, unit) requires two arguments")?;
            let unit = match args.get(1) {
                Some(Value::String(s)) => s.as_str(),
                _ => "day",
            };
            let unix = extract_unix(t).ok_or("first argument must be a time object")?;
            let dt = DateTime::from_timestamp(unix, 0).ok_or("invalid timestamp")?;
            let new_dt = match unit {
                "day" => {
                    let d = dt.date_naive();
                    DateTime::<Utc>::from_naive_utc_and_offset(d.and_hms_opt(0, 0, 0).unwrap(), Utc)
                }
                "hour" => {
                    let d = dt.date_naive();
                    DateTime::<Utc>::from_naive_utc_and_offset(
                        d.and_time(NaiveTime::from_hms_opt(dt.hour(), 0, 0).unwrap()),
                        Utc,
                    )
                }
                "month" => {
                    let d = NaiveDate::from_ymd_opt(dt.year(), dt.month(), 1).unwrap();
                    DateTime::<Utc>::from_naive_utc_and_offset(d.and_hms_opt(0, 0, 0).unwrap(), Utc)
                }
                "year" => {
                    let d = NaiveDate::from_ymd_opt(dt.year(), 1, 1).unwrap();
                    DateTime::<Utc>::from_naive_utc_and_offset(d.and_hms_opt(0, 0, 0).unwrap(), Utc)
                }
                "week" => {
                    let weekday = dt.weekday().num_days_from_monday() as i64;
                    let d = dt.date_naive() - Duration::days(weekday);
                    DateTime::<Utc>::from_naive_utc_and_offset(d.and_hms_opt(0, 0, 0).unwrap(), Utc)
                }
                "minute" => {
                    let d = dt.date_naive();
                    DateTime::<Utc>::from_naive_utc_and_offset(
                        d.and_time(NaiveTime::from_hms_opt(dt.hour(), dt.minute(), 0).unwrap()),
                        Utc,
                    )
                }
                _ => {
                    return Err(format!(
                        "unknown unit '{}'. Use: day, hour, minute, week, month, year",
                        unit
                    ))
                }
            };
            Ok(datetime_to_value(new_dt, "UTC"))
        }

        "time.end_of" => {
            let t = args
                .first()
                .ok_or("time.end_of(time_obj, unit) requires two arguments")?;
            let unit = match args.get(1) {
                Some(Value::String(s)) => s.as_str(),
                _ => "day",
            };
            let unix = extract_unix(t).ok_or("first argument must be a time object")?;
            let dt = DateTime::from_timestamp(unix, 0).ok_or("invalid timestamp")?;
            let new_dt = match unit {
                "day" => {
                    let d = dt.date_naive();
                    DateTime::<Utc>::from_naive_utc_and_offset(
                        d.and_time(NaiveTime::from_hms_opt(23, 59, 59).unwrap()),
                        Utc,
                    )
                }
                "hour" => {
                    let d = dt.date_naive();
                    DateTime::<Utc>::from_naive_utc_and_offset(
                        d.and_time(NaiveTime::from_hms_opt(dt.hour(), 59, 59).unwrap()),
                        Utc,
                    )
                }
                "month" => {
                    let (y, m) = if dt.month() == 12 {
                        (dt.year() + 1, 1)
                    } else {
                        (dt.year(), dt.month() + 1)
                    };
                    let first_of_next = NaiveDate::from_ymd_opt(y, m, 1).unwrap();
                    let last_day = first_of_next - Duration::days(1);
                    DateTime::<Utc>::from_naive_utc_and_offset(
                        last_day.and_time(NaiveTime::from_hms_opt(23, 59, 59).unwrap()),
                        Utc,
                    )
                }
                "year" => {
                    let d = NaiveDate::from_ymd_opt(dt.year(), 12, 31).unwrap();
                    DateTime::<Utc>::from_naive_utc_and_offset(
                        d.and_time(NaiveTime::from_hms_opt(23, 59, 59).unwrap()),
                        Utc,
                    )
                }
                "week" => {
                    let weekday = dt.weekday().num_days_from_monday() as i64;
                    let sunday = dt.date_naive() + Duration::days(6 - weekday);
                    DateTime::<Utc>::from_naive_utc_and_offset(
                        sunday.and_time(NaiveTime::from_hms_opt(23, 59, 59).unwrap()),
                        Utc,
                    )
                }
                "minute" => {
                    let d = dt.date_naive();
                    DateTime::<Utc>::from_naive_utc_and_offset(
                        d.and_time(NaiveTime::from_hms_opt(dt.hour(), dt.minute(), 59).unwrap()),
                        Utc,
                    )
                }
                _ => {
                    return Err(format!(
                        "unknown unit '{}'. Use: day, hour, minute, week, month, year",
                        unit
                    ))
                }
            };
            Ok(datetime_to_value(new_dt, "UTC"))
        }

        "time.sleep" => match args.first() {
            Some(Value::Int(secs)) => {
                std::thread::sleep(std::time::Duration::from_secs((*secs).max(0) as u64));
                Ok(Value::Null)
            }
            Some(Value::Float(secs)) => {
                std::thread::sleep(std::time::Duration::from_secs_f64(secs.max(0.0)));
                Ok(Value::Null)
            }
            _ => Err("time.sleep() requires seconds (number)".to_string()),
        },

        "time.measure" | "time.elapsed" => Ok(Value::Int(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as i64,
        )),

        "time.is_weekend" => {
            let unix = match args.first() {
                Some(t) => extract_unix(t).ok_or("argument must be a time object")?,
                None => Utc::now().timestamp(),
            };
            let dt = DateTime::from_timestamp(unix, 0).ok_or("invalid timestamp")?;
            let wd = dt.weekday().num_days_from_monday();
            Ok(Value::Bool(wd >= 5))
        }

        "time.is_weekday" => {
            let unix = match args.first() {
                Some(t) => extract_unix(t).ok_or("argument must be a time object")?,
                None => Utc::now().timestamp(),
            };
            let dt = DateTime::from_timestamp(unix, 0).ok_or("invalid timestamp")?;
            let wd = dt.weekday().num_days_from_monday();
            Ok(Value::Bool(wd < 5))
        }

        "time.day_of_week" => {
            let unix = match args.first() {
                Some(t) => extract_unix(t).ok_or("argument must be a time object")?,
                None => Utc::now().timestamp(),
            };
            let dt = DateTime::from_timestamp(unix, 0).ok_or("invalid timestamp")?;
            Ok(Value::String(dt.format("%A").to_string()))
        }

        "time.days_in_month" => {
            let (year, month) = match (args.first(), args.get(1)) {
                (Some(Value::Int(y)), Some(Value::Int(m))) => (*y as i32, *m as u32),
                (Some(t), _) => {
                    let unix =
                        extract_unix(t).ok_or("argument must be a time object or (year, month)")?;
                    let dt = DateTime::from_timestamp(unix, 0).ok_or("invalid timestamp")?;
                    (dt.year(), dt.month())
                }
                _ => {
                    let now = Utc::now();
                    (now.year(), now.month())
                }
            };
            let (ny, nm) = if month == 12 {
                (year + 1, 1)
            } else {
                (year, month + 1)
            };
            let first_of_next = NaiveDate::from_ymd_opt(ny, nm, 1).ok_or("invalid year/month")?;
            let last_day = first_of_next - Duration::days(1);
            Ok(Value::Int(last_day.day() as i64))
        }

        "time.is_leap_year" => {
            let year = match args.first() {
                Some(Value::Int(y)) => *y as i32,
                Some(t) => {
                    let unix =
                        extract_unix(t).ok_or("argument must be a year integer or time object")?;
                    let dt = DateTime::from_timestamp(unix, 0).ok_or("invalid timestamp")?;
                    dt.year()
                }
                None => Utc::now().year(),
            };
            let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
            Ok(Value::Bool(leap))
        }

        _ => Err(format!("unknown time function: {}", name)),
    }
}
