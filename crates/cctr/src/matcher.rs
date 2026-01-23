//! Pattern matching for test output with variable extraction and constraints.

use crate::parse::{VarType, VariableDecl};
use cctr_expr::{eval_bool, Value};
use regex::Regex;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum MatchError {
    #[error("failed to build pattern regex: {0}")]
    RegexBuild(#[from] regex::Error),
    #[error("constraint '{constraint}' failed: {error}")]
    ConstraintFailed { constraint: String, error: String },
    #[error("constraint '{constraint}' not satisfied")]
    ConstraintNotSatisfied { constraint: String },
    #[error("failed to parse JSON for variable '{name}': {error}")]
    JsonParse { name: String, error: String },
}

pub struct Matcher<'a> {
    variables: &'a [VariableDecl],
    constraints: &'a [String],
}

impl<'a> Matcher<'a> {
    pub fn new(variables: &'a [VariableDecl], constraints: &'a [String]) -> Self {
        Self {
            variables,
            constraints,
        }
    }

    pub fn matches(&self, pattern: &str, actual: &str) -> Result<bool, MatchError> {
        let regex = self.build_regex(pattern)?;

        let Some(caps) = regex.captures(actual) else {
            return Ok(false);
        };

        let values = self.extract_values(&caps)?;

        for constraint in self.constraints {
            match eval_bool(constraint, &values) {
                Ok(true) => {}
                Ok(false) => {
                    return Err(MatchError::ConstraintNotSatisfied {
                        constraint: constraint.clone(),
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

        Ok(true)
    }

    fn build_regex(&self, pattern: &str) -> Result<Regex, regex::Error> {
        let var_pattern = Regex::new(r"\{\{\s*(\w+)\s*\}\}").unwrap();

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
                    VarType::Number => r"-?\d+(?:\.\d+)?",
                    VarType::String => r".*?",
                    VarType::JsonString => r#""(?:[^"\\]|\\.)*""#,
                    VarType::JsonBool => r"true|false",
                    // Match balanced brackets - this uses a simple heuristic that works for
                    // most JSON: capture from [ to the last ] that makes the brackets balanced
                    VarType::JsonArray => r"\[[\s\S]*\]",
                    VarType::JsonObject => r"\{[\s\S]*\}",
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

        Regex::new(&regex_str)
    }

    fn extract_values(&self, caps: &regex::Captures) -> Result<HashMap<String, Value>, MatchError> {
        let mut values = HashMap::new();

        for var in self.variables {
            if let Some(m) = caps.name(&var.name) {
                let text = m.as_str();
                let value = match var.var_type {
                    VarType::Number => {
                        let n: f64 = text.parse().unwrap_or(0.0);
                        Value::Number(n)
                    }
                    VarType::String => Value::String(text.to_string()),
                    VarType::JsonString => {
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
                    VarType::JsonBool => {
                        let b = text == "true";
                        Value::Bool(b)
                    }
                    VarType::JsonArray => {
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
                    VarType::JsonObject => {
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

    fn make_var(name: &str, var_type: &str) -> VariableDecl {
        VariableDecl {
            name: name.to_string(),
            var_type: match var_type {
                "number" => VarType::Number,
                "json string" => VarType::JsonString,
                "json bool" => VarType::JsonBool,
                "json array" => VarType::JsonArray,
                "json object" => VarType::JsonObject,
                _ => VarType::String,
            },
        }
    }

    #[test]
    fn test_simple_number_match() {
        let vars = vec![make_var("n", "number")];
        let constraints = vec![];
        let matcher = Matcher::new(&vars, &constraints);

        assert!(matcher
            .matches("passed in {{ n }}s", "passed in 0.05s")
            .unwrap());
    }

    #[test]
    fn test_constraint_pass() {
        let vars = vec![make_var("n", "number")];
        let constraints = vec!["n > 0".to_string(), "n < 1".to_string()];
        let matcher = Matcher::new(&vars, &constraints);

        assert!(matcher.matches("time: {{ n }}s", "time: 0.5s").unwrap());
    }

    #[test]
    fn test_constraint_fail() {
        let vars = vec![make_var("n", "number")];
        let constraints = vec!["n < 0".to_string()];
        let matcher = Matcher::new(&vars, &constraints);

        let result = matcher.matches("time: {{ n }}s", "time: 0.5s");
        assert!(matches!(
            result,
            Err(MatchError::ConstraintNotSatisfied { .. })
        ));
    }

    #[test]
    fn test_no_match() {
        let vars = vec![make_var("n", "number")];
        let constraints = vec![];
        let matcher = Matcher::new(&vars, &constraints);

        assert!(!matcher
            .matches("passed in {{ n }}s", "failed in 0.05s")
            .unwrap());
    }

    #[test]
    fn test_empty_string_match() {
        let vars = vec![make_var("s", "string")];
        let constraints = vec!["len(s) == 0".to_string()];
        let matcher = Matcher::new(&vars, &constraints);

        assert!(matcher.matches("val: {{ s }}", "val: ").unwrap());
    }

    #[test]
    fn test_json_string_match() {
        let vars = vec![make_var("s", "json_string")];
        let constraints = vec![r#"s == "hello""#.to_string()];
        let matcher = Matcher::new(&vars, &constraints);

        assert!(matcher.matches("{{ s }}", r#""hello""#).unwrap());
    }

    #[test]
    fn test_json_string_length() {
        let vars = vec![make_var("s", "json_string")];
        let constraints = vec!["len(s) == 5".to_string()];
        let matcher = Matcher::new(&vars, &constraints);

        assert!(matcher.matches("{{ s }}", r#""hello""#).unwrap());
    }

    #[test]
    fn test_json_bool_true() {
        let vars = vec![make_var("b", "json_bool")];
        let constraints = vec!["b == true".to_string()];
        let matcher = Matcher::new(&vars, &constraints);

        assert!(matcher.matches("{{ b }}", "true").unwrap());
    }

    #[test]
    fn test_json_bool_false() {
        let vars = vec![make_var("b", "json_bool")];
        let constraints = vec!["b == false".to_string()];
        let matcher = Matcher::new(&vars, &constraints);

        assert!(matcher.matches("{{ b }}", "false").unwrap());
    }

    #[test]
    fn test_json_array_match() {
        let vars = vec![make_var("a", "json array")];
        let constraints = vec!["len(a) == 3".to_string(), "a[0] == 1".to_string()];
        let matcher = Matcher::new(&vars, &constraints);

        assert!(matcher.matches("{{ a }}", "[1, 2, 3]").unwrap());
    }

    #[test]
    fn test_json_object_match() {
        let vars = vec![make_var("o", "json object")];
        let constraints = vec![r#"o["name"] == "alice""#.to_string()];
        let matcher = Matcher::new(&vars, &constraints);

        assert!(matcher.matches("{{ o }}", r#"{"name": "alice", "age": 30}"#).unwrap());
    }

    #[test]
    fn test_json_object_dot_access() {
        let vars = vec![make_var("o", "json object")];
        let constraints = vec!["o.age == 30".to_string()];
        let matcher = Matcher::new(&vars, &constraints);

        assert!(matcher.matches("{{ o }}", r#"{"name": "alice", "age": 30}"#).unwrap());
    }

    #[test]
    fn test_json_forall() {
        let vars = vec![make_var("a", "json_array")];
        let constraints = vec!["x <= 3 forall x in a".to_string()];
        let matcher = Matcher::new(&vars, &constraints);

        assert!(matcher.matches("{{ a }}", "[1, 2, 3]").unwrap());
    }
}
