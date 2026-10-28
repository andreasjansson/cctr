//! Rendering captured values for failure reports.

use cctr_expr::Value;

/// Values longer than this many characters are truncated in failure reports
/// unless running with `-v`.
pub const MAX_INLINE_CHARS: usize = 200;

/// Single-line rendering with object keys sorted, so output is deterministic.
pub fn compact(value: &Value) -> String {
    match value {
        Value::Number(n) => format_number(*n),
        Value::String(s) => format!("{:?}", s),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
        Value::Type(t) => t.clone(),
        Value::Array(items) => {
            let items: Vec<String> = items.iter().map(compact).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Object(obj) => {
            let items: Vec<String> = sorted_entries(obj)
                .into_iter()
                .map(|(k, v)| format!("{:?}: {}", k, compact(v)))
                .collect();
            format!("{{{}}}", items.join(", "))
        }
    }
}

/// Multi-line rendering, each nested level indented two spaces further than
/// `indent`. The first line carries no indentation, so it can follow a label.
pub fn pretty(value: &Value, indent: usize) -> String {
    let pad = " ".repeat(indent + 2);
    let close = " ".repeat(indent);
    match value {
        Value::Array(items) if !items.is_empty() => {
            let items: Vec<String> = items
                .iter()
                .map(|v| format!("{}{}", pad, pretty(v, indent + 2)))
                .collect();
            format!("[\n{}\n{}]", items.join(",\n"), close)
        }
        Value::Object(obj) if !obj.is_empty() => {
            let items: Vec<String> = sorted_entries(obj)
                .into_iter()
                .map(|(k, v)| format!("{}{:?}: {}", pad, k, pretty(v, indent + 2)))
                .collect();
            format!("{{\n{}\n{}}}", items.join(",\n"), close)
        }
        _ => compact(value),
    }
}

/// Render for a failure report: inline when short, otherwise truncated, or
/// pretty-printed in full when `verbose`. Returns the text and whether it was
/// truncated.
pub fn for_report(value: &Value, indent: usize, verbose: bool) -> (String, bool) {
    let inline = compact(value);
    let len = inline.chars().count();
    if len <= MAX_INLINE_CHARS {
        return (inline, false);
    }
    if verbose {
        return (pretty(value, indent), false);
    }
    let prefix: String = inline.chars().take(MAX_INLINE_CHARS).collect();
    (
        format!("{}… (+{} chars)", prefix, len - MAX_INLINE_CHARS),
        true,
    )
}

fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{}", n)
    }
}

fn sorted_entries(obj: &std::collections::HashMap<String, Value>) -> Vec<(&String, &Value)> {
    let mut entries: Vec<_> = obj.iter().collect();
    entries.sort_by_key(|(k, _)| *k);
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn obj(pairs: &[(&str, Value)]) -> Value {
        Value::Object(
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect::<HashMap<_, _>>(),
        )
    }

    #[test]
    fn compact_sorts_keys_and_formats_numbers() {
        let v = obj(&[
            ("b", Value::Number(2.0)),
            ("a", Value::Number(1.5)),
            ("c", Value::Array(vec![Value::Null, Value::Bool(true)])),
        ]);
        assert_eq!(compact(&v), r#"{"a": 1.5, "b": 2, "c": [null, true]}"#);
    }

    #[test]
    fn pretty_indents_nested_values() {
        let v = obj(&[(
            "items",
            Value::Array(vec![Value::Number(1.0), obj(&[("k", Value::Null)])]),
        )]);
        assert_eq!(
            pretty(&v, 4),
            "{\n      \"items\": [\n        1,\n        {\n          \"k\": null\n        }\n      ]\n    }"
        );
    }

    #[test]
    fn pretty_keeps_empty_containers_inline() {
        assert_eq!(pretty(&Value::Array(vec![]), 0), "[]");
        assert_eq!(pretty(&obj(&[]), 0), "{}");
    }

    #[test]
    fn short_values_are_inline_regardless_of_verbosity() {
        let v = Value::String("short".to_string());
        assert_eq!(for_report(&v, 0, false), ("\"short\"".to_string(), false));
        assert_eq!(for_report(&v, 0, true), ("\"short\"".to_string(), false));
    }

    #[test]
    fn long_values_are_truncated_unless_verbose() {
        let v = Value::Array((0..100).map(|i| Value::Number(i as f64)).collect());
        let full_len = compact(&v).chars().count();

        let (text, truncated) = for_report(&v, 4, false);
        assert!(truncated);
        assert!(text.starts_with("[0, 1, 2"));
        assert!(text.ends_with(&format!("… (+{} chars)", full_len - MAX_INLINE_CHARS)));

        let (text, truncated) = for_report(&v, 4, true);
        assert!(!truncated);
        assert!(text.contains("\n      99\n    ]"), "{}", text);
    }

    #[test]
    fn truncation_respects_char_boundaries() {
        let v = Value::String("é".repeat(300));
        let (text, truncated) = for_report(&v, 0, false);
        assert!(truncated);
        assert!(text.starts_with("\"éé"));
    }
}
