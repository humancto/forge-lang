//! `csv`: RFC 4180 parsing and writing, shared by both engines.
//!
//! Parsing is character-level: a quoted field may contain commas, line
//! breaks (LF or CRLF) and doubled quotes, and is kept verbatim. Type
//! conversion (on by default) only touches unquoted fields that are exactly
//! an integer, a plain decimal or `true`/`false`, so nothing is lost:
//! `02134`, `+1`, `1e5` stay strings.

use crate::interpreter::Value;
use indexmap::IndexMap;

pub fn create_module() -> Value {
    let mut m = IndexMap::new();
    m.insert("parse".to_string(), Value::BuiltIn("csv.parse".to_string()));
    m.insert(
        "stringify".to_string(),
        Value::BuiltIn("csv.stringify".to_string()),
    );
    m.insert("read".to_string(), Value::BuiltIn("csv.read".to_string()));
    m.insert("write".to_string(), Value::BuiltIn("csv.write".to_string()));
    Value::Object(m)
}

pub fn call(name: &str, args: Vec<Value>) -> Result<Value, String> {
    match name {
        "csv.parse" => match args.first() {
            Some(Value::String(s)) => {
                let options = ParseOptions::from_arg(name, args.get(1))?;
                parse_csv(s, options).map_err(|e| format!("csv.parse error: {}", e))
            }
            _ => Err("csv.parse() requires a string".to_string()),
        },
        "csv.stringify" => match args.first() {
            Some(Value::Array(rows)) => Ok(Value::String(stringify_csv(rows))),
            _ => Err("csv.stringify() requires an array of objects".to_string()),
        },
        "csv.read" => match args.first() {
            Some(Value::String(path)) => {
                let options = ParseOptions::from_arg(name, args.get(1))?;
                let path = crate::stdlib::fs::confine_read(path)?;
                let content =
                    std::fs::read_to_string(path).map_err(|e| format!("csv.read error: {}", e))?;
                parse_csv(&content, options).map_err(|e| format!("csv.read error: {}", e))
            }
            _ => Err("csv.read() requires a file path".to_string()),
        },
        "csv.write" => match (args.first(), args.get(1)) {
            (Some(Value::String(path)), Some(Value::Array(rows))) => {
                let path = crate::stdlib::fs::confine_write(path)?;
                let content = stringify_csv(rows);
                std::fs::write(path, &content)
                    .map(|_| Value::Null)
                    .map_err(|e| format!("csv.write error: {}", e))
            }
            _ => Err("csv.write() requires (path, array)".to_string()),
        },
        _ => Err(format!("unknown csv function: {}", name)),
    }
}

/// `csv.parse(text, { header, types })` / `csv.read(path, { ... })`.
#[derive(Debug, Clone, Copy)]
struct ParseOptions {
    /// The first record names the fields (rows are objects); otherwise
    /// every record is an array of fields.
    header: bool,
    /// Convert unquoted fields that are exactly an integer, a decimal or
    /// `true`/`false` (see [`infer`]); otherwise every field is a string.
    types: bool,
}

impl Default for ParseOptions {
    fn default() -> Self {
        ParseOptions {
            header: true,
            types: true,
        }
    }
}

impl ParseOptions {
    fn from_arg(function: &str, arg: Option<&Value>) -> Result<Self, String> {
        let mut options = ParseOptions::default();
        let map = match arg {
            None | Some(Value::Null) => return Ok(options),
            Some(Value::Object(map)) => map,
            Some(_) => {
                return Err(format!(
                    "{}() options must be an object like {{ header: false, types: false }}",
                    function
                ))
            }
        };
        for (key, value) in map {
            let flag = match value {
                Value::Bool(b) => *b,
                _ => {
                    return Err(format!(
                        "{}() option '{}' must be true or false",
                        function, key
                    ))
                }
            };
            match key.as_str() {
                "header" => options.header = flag,
                "types" => options.types = flag,
                other => {
                    return Err(format!(
                        "{}() has no option '{}' (options: header, types)",
                        function, other
                    ))
                }
            }
        }
        Ok(options)
    }
}

/// One parsed field: its text, and whether it was quoted (quoted fields
/// are kept verbatim and never converted).
#[derive(Debug, Clone, PartialEq)]
struct Field {
    text: String,
    quoted: bool,
}

fn is_blank(ch: char) -> bool {
    ch == ' ' || ch == '\t'
}

/// Split `input` into records of fields (RFC 4180): fields are separated by
/// `,` and records by LF or CRLF; a field that starts with `"` is quoted
/// and may contain commas, line breaks and doubled quotes (`""`), all kept
/// verbatim. Whitespace around an unquoted field, or around the quotes of
/// a quoted one, is not part of the field. Blank lines are skipped.
fn parse_records(input: &str) -> Result<Vec<Vec<Field>>, String> {
    let input = input.strip_prefix('\u{feff}').unwrap_or(input);
    let mut records = Vec::new();
    let mut record: Vec<Field> = Vec::new();
    let mut chars = input.chars().peekable();
    let mut line = 1usize;
    loop {
        // One field.
        while chars.peek().is_some_and(|&ch| is_blank(ch)) {
            chars.next();
        }
        let mut text = String::new();
        let quoted = chars.peek() == Some(&'"');
        if quoted {
            chars.next();
            let start_line = line;
            loop {
                match chars.next() {
                    Some('"') if chars.peek() == Some(&'"') => {
                        chars.next();
                        text.push('"');
                    }
                    Some('"') => break,
                    Some(ch) => {
                        if ch == '\n' {
                            line += 1;
                        }
                        text.push(ch);
                    }
                    None => {
                        return Err(format!(
                            "unterminated quoted field starting on line {}",
                            start_line
                        ))
                    }
                }
            }
        }
        // The rest of the field: all of an unquoted one, or anything
        // between a closing quote and the separator (`"a" b` keeps ` b`
        // without its trailing whitespace).
        let rest_start = text.len();
        while let Some(&ch) = chars.peek() {
            if ch == ',' || ch == '\n' || ch == '\r' {
                break;
            }
            text.push(ch);
            chars.next();
        }
        let kept = rest_start + text[rest_start..].trim_end_matches(is_blank).len();
        text.truncate(kept);
        record.push(Field { text, quoted });

        // What ends the field.
        match chars.next() {
            Some(',') => {}
            Some(end) => {
                if end == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                line += 1;
                push_record(&mut records, std::mem::take(&mut record));
            }
            None => {
                push_record(&mut records, record);
                return Ok(records);
            }
        }
    }
}

/// Keep a record unless it is a blank line (one empty unquoted field).
fn push_record(records: &mut Vec<Vec<Field>>, record: Vec<Field>) {
    let blank = record.len() == 1 && !record[0].quoted && record[0].text.is_empty();
    if !blank {
        records.push(record);
    }
}

/// The value of an unquoted field when type conversion is on: an integer
/// or a decimal number written in its plain form (`42`, `-7`, `3.25`),
/// `true` or `false`. Anything else stays a string, so no information is
/// lost: `02134`, `+1`, `-0`, `1e5`, `1.`, `.5`, `NaN` and integers beyond
/// the 64-bit range are kept as written.
fn infer(text: &str) -> Value {
    match text {
        "true" => return Value::Bool(true),
        "false" => return Value::Bool(false),
        _ => {}
    }
    let keep = || Value::String(text.to_string());
    let unsigned = text.strip_prefix('-').unwrap_or(text);
    let (int_part, frac_part) = match unsigned.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (unsigned, None),
    };
    let all_digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !all_digits(int_part) || (int_part.len() > 1 && int_part.starts_with('0')) {
        return keep();
    }
    match frac_part {
        None => match text.parse::<i64>() {
            // Only the canonical spelling (`-0` is not one).
            Ok(n) if n.to_string() == text => Value::Int(n),
            _ => keep(),
        },
        Some(f) if all_digits(f) => match text.parse::<f64>() {
            Ok(x) if x.is_finite() => Value::Float(x),
            _ => keep(),
        },
        Some(_) => keep(),
    }
}

fn field_value(field: &Field, options: ParseOptions) -> Value {
    if options.types && !field.quoted {
        infer(&field.text)
    } else {
        Value::String(field.text.clone())
    }
}

fn parse_csv(input: &str, options: ParseOptions) -> Result<Value, String> {
    let records = parse_records(input)?;
    if !options.header {
        return Ok(Value::Array(
            records
                .iter()
                .map(|r| Value::Array(r.iter().map(|f| field_value(f, options)).collect()))
                .collect(),
        ));
    }
    let mut records = records.into_iter();
    let headers: Vec<String> = match records.next() {
        Some(h) => h.into_iter().map(|f| f.text).collect(),
        None => return Ok(Value::Array(Vec::new())),
    };
    let missing = Field {
        text: String::new(),
        quoted: false,
    };
    let rows = records
        .map(|values| {
            let mut map = IndexMap::new();
            for (i, header) in headers.iter().enumerate() {
                let field = values.get(i).unwrap_or(&missing);
                map.insert(header.clone(), field_value(field, options));
            }
            Value::Object(map)
        })
        .collect();
    Ok(Value::Array(rows))
}

/// One field as CSV text: quoted when it contains a separator, a quote or
/// a line break, has surrounding whitespace, or is a string that parsing
/// would convert (`"42"`, `"true"`), so `csv.parse(csv.stringify(rows))`
/// gives the rows back.
fn csv_field(value: &Value) -> String {
    let text = format!("{}", value);
    let needs_quotes = text.contains([',', '"', '\n', '\r'])
        || text.starts_with(is_blank)
        || text.ends_with(is_blank)
        || (matches!(value, Value::String(_)) && !matches!(infer(&text), Value::String(_)));
    if needs_quotes {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text
    }
}

fn stringify_csv(rows: &[Value]) -> String {
    if rows.is_empty() {
        return String::new();
    }

    let headers: Vec<String> = if let Some(Value::Object(first)) = rows.first() {
        first.keys().cloned().collect()
    } else {
        return String::new();
    };

    let header_line: Vec<String> = headers
        .iter()
        .map(|h| csv_field(&Value::String(h.clone())))
        .collect();
    let mut output = header_line.join(",");
    output.push('\n');

    for row in rows {
        if let Value::Object(map) = row {
            let values: Vec<String> = headers
                .iter()
                .map(|h| map.get(h).map(csv_field).unwrap_or_default())
                .collect();
            output.push_str(&values.join(","));
            output.push('\n');
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(line: &str) -> Vec<String> {
        let records = parse_records(line).expect("parses");
        records
            .into_iter()
            .next()
            .unwrap_or_default()
            .into_iter()
            .map(|f| f.text)
            .collect()
    }

    fn rows(input: &str) -> Vec<IndexMap<String, Value>> {
        match parse_csv(input, ParseOptions::default()).expect("parses") {
            Value::Array(rows) => rows
                .into_iter()
                .map(|r| match r {
                    Value::Object(m) => m,
                    other => panic!("expected object row, got {:?}", other),
                })
                .collect(),
            other => panic!("expected array, got {:?}", other),
        }
    }

    fn s(text: &str) -> Value {
        Value::String(text.to_string())
    }

    #[test]
    fn parse_csv_line_simple() {
        assert_eq!(fields("a,b,c"), vec!["a", "b", "c"]);
    }

    #[test]
    fn parse_csv_line_quoted_with_comma() {
        assert_eq!(
            fields(r#""Smith, John",42,true"#),
            vec!["Smith, John", "42", "true"]
        );
    }

    #[test]
    fn parse_csv_line_escaped_quotes() {
        assert_eq!(
            fields(r#""He said ""hello""",done"#),
            vec![r#"He said "hello""#, "done"]
        );
    }

    #[test]
    fn parse_csv_line_empty_fields() {
        assert_eq!(fields("a,,c"), vec!["a", "", "c"]);
        assert_eq!(fields("a,b,"), vec!["a", "b", ""]);
    }

    #[test]
    fn parse_csv_line_crlf() {
        assert_eq!(fields("a,b,c\r\n"), vec!["a", "b", "c"]);
    }

    #[test]
    fn parse_csv_line_quoted_empty() {
        assert_eq!(fields(r#""",hello"#), vec!["", "hello"]);
    }

    #[test]
    fn parse_csv_line_single_field() {
        assert_eq!(fields("hello"), vec!["hello"]);
    }

    #[test]
    fn unquoted_whitespace_is_trimmed_quoted_is_kept() {
        assert_eq!(fields("  a  , b , c "), vec!["a", "b", "c"]);
        assert_eq!(fields(r#" "  Ann  " ,x"#), vec!["  Ann  ", "x"]);
        assert_eq!(fields("a b,c"), vec!["a b", "c"]);
    }

    #[test]
    fn quoted_fields_span_line_breaks() {
        let input = "name,note,zip\r\nBob,\"multi\nline\",00501\r\n\"Ann\",\"a\r\nb\",1\n";
        let r = rows(input);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].get("note"), Some(&s("multi\nline")));
        assert_eq!(r[0].get("zip"), Some(&s("00501")));
        assert_eq!(r[1].get("name"), Some(&s("Ann")));
        assert_eq!(r[1].get("note"), Some(&s("a\r\nb")));
        assert_eq!(r[1].get("zip"), Some(&Value::Int(1)));
    }

    #[test]
    fn finding_example_parses_exactly() {
        let input = "name,qty,note,zip\n\"Smith, J\",3,\"said \"\"hi\"\"\",02134\nBob,x,\"multi\nline\",00501\n";
        let r = rows(input);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].get("name"), Some(&s("Smith, J")));
        assert_eq!(r[0].get("qty"), Some(&Value::Int(3)));
        assert_eq!(r[0].get("note"), Some(&s("said \"hi\"")));
        assert_eq!(r[0].get("zip"), Some(&s("02134")));
        assert_eq!(r[1].get("qty"), Some(&s("x")));
        assert_eq!(r[1].get("note"), Some(&s("multi\nline")));
        assert_eq!(r[1].get("zip"), Some(&s("00501")));
    }

    #[test]
    fn inference_never_loses_information() {
        assert_eq!(infer("42"), Value::Int(42));
        assert_eq!(infer("-7"), Value::Int(-7));
        assert_eq!(infer("0"), Value::Int(0));
        assert_eq!(infer("3.25"), Value::Float(3.25));
        assert_eq!(infer("-0.5"), Value::Float(-0.5));
        assert_eq!(infer("1.50"), Value::Float(1.5));
        assert_eq!(infer("true"), Value::Bool(true));
        assert_eq!(infer("false"), Value::Bool(false));
        for kept in [
            "02134",
            "007",
            "+1",
            "-0",
            "1e5",
            "1.",
            ".5",
            "1.2.3",
            "00.5",
            "NaN",
            "inf",
            "True",
            "99999999999999999999",
            "12a",
            "",
            "-",
        ] {
            assert_eq!(infer(kept), s(kept), "{:?} must stay a string", kept);
        }
    }

    #[test]
    fn quoted_fields_are_never_converted() {
        let r = rows("a,b\n\"42\",\"true\"\n");
        assert_eq!(r[0].get("a"), Some(&s("42")));
        assert_eq!(r[0].get("b"), Some(&s("true")));
    }

    #[test]
    fn blank_lines_are_skipped_but_empty_quoted_records_are_not() {
        let r = rows("a\n\n1\n\n\"\"\n");
        assert_eq!(r.len(), 2);
        assert_eq!(r[1].get("a"), Some(&s("")));
    }

    #[test]
    fn missing_fields_are_empty_strings() {
        let r = rows("a,b,c\n1\n");
        assert_eq!(r[0].get("b"), Some(&s("")));
        assert_eq!(r[0].get("c"), Some(&s("")));
    }

    #[test]
    fn unterminated_quote_is_an_error() {
        let err = parse_csv("a\n\"open\n", ParseOptions::default()).expect_err("must fail");
        assert!(
            err.contains("unterminated quoted field starting on line 2"),
            "{}",
            err
        );
    }

    #[test]
    fn options_select_headerless_and_untyped() {
        let opts = ParseOptions {
            header: false,
            types: false,
        };
        let v = parse_csv("a,1\nb,2\n", opts).expect("parses");
        assert_eq!(
            v,
            Value::Array(vec![
                Value::Array(vec![s("a"), s("1")]),
                Value::Array(vec![s("b"), s("2")]),
            ])
        );
        let mut bad = IndexMap::new();
        bad.insert("headers".to_string(), Value::Bool(false));
        let err = call("csv.parse", vec![s("a"), Value::Object(bad)]).expect_err("unknown option");
        assert!(err.contains("no option 'headers'"), "{}", err);
    }

    #[test]
    fn byte_order_mark_is_ignored() {
        let r = rows("\u{feff}id\n1\n");
        assert_eq!(r[0].get("id"), Some(&Value::Int(1)));
    }

    #[test]
    fn parse_csv_full_rfc4180() {
        let r = rows("name,age,city\n\"Smith, John\",42,\"New York\"\nJane,30,Boston");
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].get("name"), Some(&s("Smith, John")));
        assert_eq!(r[0].get("age"), Some(&Value::Int(42)));
        assert_eq!(r[0].get("city"), Some(&s("New York")));
    }

    #[test]
    fn stringify_csv_round_trip() {
        let input = "name,score\nAlice,100\nBob,95";
        let parsed = parse_csv(input, ParseOptions::default()).expect("parses");
        if let Value::Array(ref rows) = parsed {
            let output = stringify_csv(rows);
            assert!(output.contains("name,score"));
            assert!(output.contains("Alice,100"));
            assert!(output.contains("Bob,95"));
        }
    }

    #[test]
    fn stringify_csv_with_commas() {
        // Values containing commas should be quoted in output
        let mut row = IndexMap::new();
        row.insert("name".to_string(), Value::String("Smith, John".into()));
        row.insert("age".to_string(), Value::Int(42));
        let rows = vec![Value::Object(row)];
        let output = stringify_csv(&rows);
        assert!(output.contains("\"Smith, John\""), "got: {}", output);
    }

    #[test]
    fn stringify_then_parse_gives_the_rows_back() {
        let mut row = IndexMap::new();
        row.insert("note".to_string(), s("multi\nline \"q\""));
        row.insert("zip".to_string(), s("02134"));
        row.insert("id".to_string(), s("42"));
        row.insert("pad".to_string(), s(" x "));
        row.insert("n".to_string(), Value::Int(7));
        let original = vec![Value::Object(row)];
        let text = stringify_csv(&original);
        assert_eq!(
            parse_csv(&text, ParseOptions::default()).expect("parses"),
            Value::Array(original)
        );
    }
}
