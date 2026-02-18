//! Pattern matching for test output with variable extraction and constraints.

use crate::{VarType, VariableDecl};
use cctr_expr::{eval_bool, Value};
use regex::Regex;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MatchError {
    #[error("failed to build pattern regex: {0}")]
    RegexBuild(#[from] regex::Error),
    #[error("duplicate variable '{{{{ {0} }}}}' in pattern - each variable can only appear once")]
    DuplicateVariable(String),
    #[error("constraint '{constraint}' failed: {error}")]
    ConstraintFailed { constraint: String, error: String },
    #[error("{}", format_constraint_error(.constraint, .bindings))]
    ConstraintNotSatisfied {
        constraint: String,
        bindings: Vec<(String, String)>,
    },
    #[error("failed to parse JSON for variable '{name}': {error}")]
    JsonParse { name: String, error: String },
}

fn format_constraint_error(constraint: &str, bindings: &[(String, String)]) -> String {
    let mut msg = format!("constraint '{}' not satisfied", constraint);
    if !bindings.is_empty() {
        msg.push_str("\n  where ");
        let binding_strs: Vec<String> = bindings
            .iter()
            .map(|(name, value)| format!("{} = {}", name, value))
            .collect();
        msg.push_str(&binding_strs.join(", "));
    }
    msg
}

fn format_value(value: &Value) -> String {
    match value {
        Value::Number(n) => {
            if n.fract() == 0.0 && n.abs() < 1e15 {
                format!("{}", *n as i64)
            } else {
                format!("{}", n)
            }
        }
        Value::String(s) => format!("{:?}", s),
        Value::Bool(b) => format!("{}", b),
        Value::Null => "null".to_string(),
        Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(format_value).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Object(obj) => {
            let mut pairs: Vec<(&String, &Value)> = obj.iter().collect();
            pairs.sort_by_key(|(k, _)| *k);
            let items: Vec<String> = pairs
                .iter()
                .map(|(k, v)| format!("{:?}: {}", k, format_value(v)))
                .collect();
            format!("{{{}}}", items.join(", "))
        }
        Value::Type(t) => t.clone(),
    }
}

/// Duck-type a captured string value into the appropriate Value type.
/// Priority: json object > json array > json string > json bool > number > string
fn duck_type_value(text: &str) -> Value {
    let trimmed = text.trim();

    // Try JSON object
    if trimmed.starts_with('{') {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Ok(v) = json_to_value(&json) {
                return v;
            }
        }
    }

    // Try JSON array
    if trimmed.starts_with('[') {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(trimmed) {
            if let Ok(v) = json_to_value(&json) {
                return v;
            }
        }
    }

    // Try JSON string
    if trimmed.starts_with('"') {
        if let Ok(serde_json::Value::String(s)) = serde_json::from_str::<serde_json::Value>(trimmed)
        {
            return Value::String(s);
        }
    }

    // Try JSON bool
    if trimmed == "true" {
        return Value::Bool(true);
    }
    if trimmed == "false" {
        return Value::Bool(false);
    }

    // Try null
    if trimmed == "null" {
        return Value::Null;
    }

    // Try number (reject infinity/nan which aren't valid JSON)
    if let Ok(n) = trimmed.parse::<f64>() {
        if n.is_finite() {
            return Value::Number(n);
        }
    }

    // Fall back to string
    Value::String(text.to_string())
}

pub struct MatchResult {
    pub matched: bool,
    pub captured: HashMap<String, Value>,
}

pub struct Matcher<'a> {
    variables: &'a [VariableDecl],
    constraints: &'a [String],
    env_vars: &'a [(String, String)],
}

impl<'a> Matcher<'a> {
    pub fn new(
        variables: &'a [VariableDecl],
        constraints: &'a [String],
        env_vars: &'a [(String, String)],
    ) -> Self {
        Self {
            variables,
            constraints,
            env_vars,
        }
    }

    pub fn matches(
        &self,
        pattern: &str,
        actual: &str,
        prior_vars: &HashMap<String, Value>,
    ) -> Result<MatchResult, MatchError> {
        let clean_pattern = self.strip_type_annotations(pattern);
        let regex = self.build_regex(&clean_pattern)?;

        let Some(caps) = regex.captures(actual) else {
            return Ok(MatchResult {
                matched: false,
                captured: HashMap::new(),
            });
        };

        // Set CCTR_* env vars so env() function can access them
        for (key, value) in self.env_vars {
            std::env::set_var(key, value);
        }

        let captured = self.extract_values(&caps)?;

        // Merge prior variables with newly captured ones (new values override)
        let mut all_values = prior_vars.clone();
        all_values.extend(captured.clone());

        let bindings = self.format_all_bindings(&all_values);

        for constraint in self.constraints {
            match eval_bool(constraint, &all_values) {
                Ok(true) => {}
                Ok(false) => {
                    return Err(MatchError::ConstraintNotSatisfied {
                        constraint: constraint.clone(),
                        bindings: bindings.clone(),
                    });
                }
                Err(e) => {
                    return Err(MatchError::ConstraintFailed {
                        constraint: constraint.clone(),
                        error: e.to_string(),
                    });
                }
            }
        }

        Ok(MatchResult {
            matched: true,
            captured,
        })
    }

    /// Strip type annotations from placeholders: {{ x: number }} -> {{ x }}
    fn strip_type_annotations(&self, pattern: &str) -> String {
        let re = Regex::new(r"\{\{\s*(\w+)\s*:\s*[^}]+\}\}").unwrap();
        re.replace_all(pattern, "{{ $1 }}").to_string()
    }

    fn format_all_bindings(&self, values: &HashMap<String, Value>) -> Vec<(String, String)> {
        let mut bindings: Vec<_> = values
            .iter()
            .map(|(name, v)| (name.clone(), format_value(v)))
            .collect();
        bindings.sort_by(|a, b| a.0.cmp(&b.0));
        bindings
    }

    fn build_regex(&self, pattern: &str) -> Result<Regex, MatchError> {
        let var_pattern = Regex::new(r"\{\{\s*(\w+)\s*\}\}").unwrap();

        // Check for duplicate variable names
        let mut seen_vars = std::collections::HashSet::new();
        for cap in var_pattern.captures_iter(pattern) {
            let var_name = cap.get(1).unwrap().as_str();
            if self.variables.iter().any(|v| v.name == var_name) && !seen_vars.insert(var_name) {
                return Err(MatchError::DuplicateVariable(var_name.to_string()));
            }
        }

        let mut regex_str = String::new();
        let mut last_end = 0;

        for cap in var_pattern.captures_iter(pattern) {
            let full_match = cap.get(0).unwrap();
            let var_name = cap.get(1).unwrap().as_str();

            let literal = &pattern[last_end..full_match.start()];
            regex_str.push_str(&regex::escape(literal));

            if let Some(var) = self.variables.iter().find(|v| v.name == var_name) {
                // For JSON types, we use a greedy approach that captures balanced brackets/braces.
                // The actual JSON validation happens in extract_values via serde_json.
                let capture_pattern = match var.var_type {
                    Some(VarType::Number) => r"-?\d+(?:\.\d+)?",
                    Some(VarType::String) => r".*?",
                    Some(VarType::JsonString) => r#""(?:[^"\\]|\\.)*""#,
                    Some(VarType::JsonBool) => r"true|false",
                    Some(VarType::JsonArray) => r"\[[\s\S]*\]",
                    Some(VarType::JsonObject) => r"\{[\s\S]*\}",
                    // Duck-typed: match anything (greedy but stops at next literal)
                    None => r".*?",
                };
                regex_str.push_str(&format!("(?P<{}>{})", var_name, capture_pattern));
            } else {
                regex_str.push_str(&regex::escape(
                    &pattern[full_match.start()..full_match.end()],
                ));
            }

            last_end = full_match.end();
        }

        regex_str.push_str(&regex::escape(&pattern[last_end..]));
        let regex_str = format!("(?s)^{}$", regex_str);

        Ok(Regex::new(&regex_str)?)
    }

    fn extract_values(&self, caps: &regex::Captures) -> Result<HashMap<String, Value>, MatchError> {
        let mut values = HashMap::new();

        for var in self.variables {
            if let Some(m) = caps.name(&var.name) {
                let text = m.as_str();
                let value = match var.var_type {
                    Some(VarType::Number) => {
                        let n: f64 = text.parse().unwrap_or(0.0);
                        Value::Number(n)
                    }
                    Some(VarType::String) => Value::String(text.to_string()),
                    Some(VarType::JsonString) => {
                        let json: serde_json::Value =
                            serde_json::from_str(text).map_err(|e| MatchError::JsonParse {
                                name: var.name.clone(),
                                error: e.to_string(),
                            })?;
                        match json {
                            serde_json::Value::String(s) => Value::String(s),
                            _ => {
                                return Err(MatchError::JsonParse {
                                    name: var.name.clone(),
                                    error: "expected JSON string".to_string(),
                                })
                            }
                        }
                    }
                    Some(VarType::JsonBool) => {
                        let b = text == "true";
                        Value::Bool(b)
                    }
                    Some(VarType::JsonArray) => {
                        let json: serde_json::Value =
                            serde_json::from_str(text).map_err(|e| MatchError::JsonParse {
                                name: var.name.clone(),
                                error: e.to_string(),
                            })?;
                        json_to_value(&json).map_err(|e| MatchError::JsonParse {
                            name: var.name.clone(),
                            error: e,
                        })?
                    }
                    Some(VarType::JsonObject) => {
                        let json: serde_json::Value =
                            serde_json::from_str(text).map_err(|e| MatchError::JsonParse {
                                name: var.name.clone(),
                                error: e.to_string(),
                            })?;
                        json_to_value(&json).map_err(|e| MatchError::JsonParse {
                            name: var.name.clone(),
                            error: e,
                        })?
                    }
                    // Duck-typed: infer from value
                    None => duck_type_value(text),
                };
                values.insert(var.name.clone(), value);
            }
        }

        Ok(values)
    }
}

fn json_to_value(json: &serde_json::Value) -> Result<Value, String> {
    match json {
        serde_json::Value::Null => Ok(Value::Null),
        serde_json::Value::Bool(b) => Ok(Value::Bool(*b)),
        serde_json::Value::Number(n) => Ok(Value::Number(n.as_f64().unwrap_or(0.0))),
        serde_json::Value::String(s) => Ok(Value::String(s.clone())),
        serde_json::Value::Array(arr) => {
            let items: Result<Vec<_>, _> = arr.iter().map(json_to_value).collect();
            Ok(Value::Array(items?))
        }
        serde_json::Value::Object(obj) => {
            let mut map = HashMap::new();
            for (k, v) in obj {
                map.insert(k.clone(), json_to_value(v)?);
            }
            Ok(Value::Object(map))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_var(name: &str, var_type: Option<&str>) -> VariableDecl {
        VariableDecl {
            name: name.to_string(),
            var_type: var_type.map(|t| match t {
                "number" => VarType::Number,
                "json string" => VarType::JsonString,
                "json bool" => VarType::JsonBool,
                "json array" => VarType::JsonArray,
                "json object" => VarType::JsonObject,
                _ => VarType::String,
            }),
        }
    }

    #[test]
    fn test_simple_number_match() {
        let vars = vec![make_var("n", Some("number"))];
        let constraints = vec![];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(matcher
            .matches("passed in {{ n }}s", "passed in 0.05s")
            .unwrap());
    }

    #[test]
    fn test_constraint_pass() {
        let vars = vec![make_var("n", Some("number"))];
        let constraints = vec!["n > 0".to_string(), "n < 1".to_string()];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(matcher.matches("time: {{ n }}s", "time: 0.5s").unwrap());
    }

    #[test]
    fn test_constraint_fail() {
        let vars = vec![make_var("n", Some("number"))];
        let constraints = vec!["n < 0".to_string()];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        let result = matcher.matches("time: {{ n }}s", "time: 0.5s");
        assert!(matches!(
            result,
            Err(MatchError::ConstraintNotSatisfied { .. })
        ));
    }

    #[test]
    fn test_no_match() {
        let vars = vec![make_var("n", Some("number"))];
        let constraints = vec![];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(!matcher
            .matches("passed in {{ n }}s", "failed in 0.05s")
            .unwrap());
    }

    #[test]
    fn test_empty_string_match() {
        let vars = vec![make_var("s", Some("string"))];
        let constraints = vec!["len(s) == 0".to_string()];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(matcher.matches("val: {{ s }}", "val: ").unwrap());
    }

    #[test]
    fn test_json_string_match() {
        let vars = vec![make_var("s", Some("json string"))];
        let constraints = vec![r#"s == "hello""#.to_string()];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(matcher.matches("{{ s }}", r#""hello""#).unwrap());
    }

    #[test]
    fn test_json_string_length() {
        let vars = vec![make_var("s", Some("json string"))];
        let constraints = vec!["len(s) == 5".to_string()];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(matcher.matches("{{ s }}", r#""hello""#).unwrap());
    }

    #[test]
    fn test_json_bool_true() {
        let vars = vec![make_var("b", Some("json bool"))];
        let constraints = vec!["b == true".to_string()];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(matcher.matches("{{ b }}", "true").unwrap());
    }

    #[test]
    fn test_json_bool_false() {
        let vars = vec![make_var("b", Some("json bool"))];
        let constraints = vec!["b == false".to_string()];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(matcher.matches("{{ b }}", "false").unwrap());
    }

    #[test]
    fn test_json_array_match() {
        let vars = vec![make_var("a", Some("json array"))];
        let constraints = vec!["len(a) == 3".to_string(), "a[0] == 1".to_string()];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(matcher.matches("{{ a }}", "[1, 2, 3]").unwrap());
    }

    #[test]
    fn test_json_object_match() {
        let vars = vec![make_var("o", Some("json object"))];
        let constraints = vec![r#"o["name"] == "alice""#.to_string()];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(matcher
            .matches("{{ o }}", r#"{"name": "alice", "age": 30}"#)
            .unwrap());
    }

    #[test]
    fn test_json_object_dot_access() {
        let vars = vec![make_var("o", Some("json object"))];
        let constraints = vec!["o.age == 30".to_string()];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(matcher
            .matches("{{ o }}", r#"{"name": "alice", "age": 30}"#)
            .unwrap());
    }

    #[test]
    fn test_json_forall() {
        let vars = vec![make_var("a", Some("json array"))];
        let constraints = vec!["x <= 3 forall x in a".to_string()];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(matcher.matches("{{ a }}", "[1, 2, 3]").unwrap());
    }

    #[test]
    fn test_duck_typed_number() {
        let vars = vec![make_var("x", None)];
        let constraints = vec!["x > 0".to_string()];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(matcher.matches("val: {{ x }}", "val: 42").unwrap());
    }

    #[test]
    fn test_duck_typed_string() {
        let vars = vec![make_var("x", None)];
        let constraints = vec!["len(x) == 5".to_string()];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(matcher.matches("val: {{ x }}", "val: hello").unwrap());
    }

    #[test]
    fn test_duck_typed_bool() {
        let vars = vec![make_var("x", None)];
        let constraints = vec!["x == true".to_string()];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(matcher.matches("val: {{ x }}", "val: true").unwrap());
    }

    #[test]
    fn test_inline_type_annotation_stripped() {
        let vars = vec![make_var("n", Some("number"))];
        let constraints = vec![];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        // The pattern has inline type annotation which should be stripped
        assert!(matcher.matches("val: {{ n: number }}", "val: 42").unwrap());
    }
}
