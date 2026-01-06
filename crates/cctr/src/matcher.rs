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

        let values = self.extract_values(&caps);

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
                let capture_pattern = match var.var_type {
                    VarType::Number => r"-?\d+(?:\.\d+)?",
                    VarType::String => r".*?",
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

    fn extract_values(&self, caps: &regex::Captures) -> HashMap<String, Value> {
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
                };
                values.insert(var.name.clone(), value);
            }
        }

        values
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
}
