//! Pattern matching with template variables and constraints.

use crate::expr::{eval_bool, EvalError, Value};
use regex::Regex;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VarType {
    Number,
    String,
}

#[derive(Error, Debug)]
pub enum MatchError {
    #[error("failed to build pattern regex: {0}")]
    RegexBuild(#[from] regex::Error),
    #[error("constraint '{constraint}' failed to evaluate: {error}")]
    ConstraintFailed {
        constraint: String,
        error: EvalError,
    },
    #[error("constraint '{constraint}' not satisfied")]
    ConstraintNotSatisfied { constraint: String },
}

#[derive(Debug, Clone)]
pub enum MatchResult {
    Match(HashMap<String, Value>),
    NoMatch,
}

#[derive(Debug, Clone)]
struct Variable {
    name: String,
    var_type: VarType,
}

/// A pattern matcher with template variables and constraints.
///
/// # Example
///
/// ```
/// use cctr_match::{Pattern, VarType};
///
/// let pattern = Pattern::new("Completed in {{ time }}s")
///     .var("time", VarType::Number)
///     .constraint("time > 0")
///     .constraint("time < 60");
///
/// assert!(pattern.matches("Completed in 1.5s").unwrap());
/// assert!(!pattern.matches("Completed in 120s").unwrap()); // constraint fails
/// ```
#[derive(Debug, Clone)]
pub struct Pattern {
    template: String,
    variables: Vec<Variable>,
    constraints: Vec<String>,
}

impl Pattern {
    pub fn new(template: impl Into<String>) -> Self {
        Self {
            template: template.into(),
            variables: Vec::new(),
            constraints: Vec::new(),
        }
    }

    pub fn var(mut self, name: impl Into<String>, var_type: VarType) -> Self {
        self.variables.push(Variable {
            name: name.into(),
            var_type,
        });
        self
    }

    pub fn constraint(mut self, constraint: impl Into<String>) -> Self {
        self.constraints.push(constraint.into());
        self
    }

    pub fn matches(&self, actual: &str) -> Result<bool, MatchError> {
        match self.match_extract(actual)? {
            MatchResult::Match(_) => Ok(true),
            MatchResult::NoMatch => Ok(false),
        }
    }

    pub fn match_extract(&self, actual: &str) -> Result<MatchResult, MatchError> {
        let regex = self.build_regex()?;

        let Some(caps) = regex.captures(actual) else {
            return Ok(MatchResult::NoMatch);
        };

        let captured = self.extract_values(&caps);

        for constraint in &self.constraints {
            match eval_bool(constraint, &captured) {
                Ok(true) => {}
                Ok(false) => {
                    return Err(MatchError::ConstraintNotSatisfied {
                        constraint: constraint.clone(),
                    });
                }
                Err(e) => {
                    return Err(MatchError::ConstraintFailed {
                        constraint: constraint.clone(),
                        error: e,
                    });
                }
            }
        }

        Ok(MatchResult::Match(captured))
    }

    fn build_regex(&self) -> Result<Regex, regex::Error> {
        let var_pattern = Regex::new(r"\{\{\s*(\w+)\s*\}\}").unwrap();

        let mut regex_str = String::new();
        let mut last_end = 0;

        for cap in var_pattern.captures_iter(&self.template) {
            let full_match = cap.get(0).unwrap();
            let var_name = cap.get(1).unwrap().as_str();

            // Escape the literal text before this variable
            let literal = &self.template[last_end..full_match.start()];
            regex_str.push_str(&regex::escape(literal));

            // Add capture group for the variable
            if let Some(var) = self.variables.iter().find(|v| v.name == var_name) {
                let capture_pattern = match var.var_type {
                    VarType::Number => r"-?\d+(?:\.\d+)?",
                    VarType::String => r".+?",
                };
                regex_str.push_str(&format!("(?P<{}>{})", var_name, capture_pattern));
            } else {
                // Not a declared variable - treat as literal {{ var }}
                regex_str.push_str(&regex::escape(
                    &self.template[full_match.start()..full_match.end()],
                ));
            }

            last_end = full_match.end();
        }

        // Add remaining literal text
        regex_str.push_str(&regex::escape(&self.template[last_end..]));

        // Use dotall mode for multiline matching
        let regex_str = format!("(?s)^{}$", regex_str);

        Regex::new(&regex_str)
    }

    fn extract_values(&self, caps: &regex::Captures) -> HashMap<String, Value> {
        let mut values = HashMap::new();

        for var in &self.variables {
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

    #[test]
    fn test_simple_number_match() {
        let pattern = Pattern::new("passed in {{ n }}s").var("n", VarType::Number);
        assert!(pattern.matches("passed in 0.05s").unwrap());
    }

    #[test]
    fn test_multiple_numbers() {
        let pattern = Pattern::new("{{ a }} + {{ b }} = result")
            .var("a", VarType::Number)
            .var("b", VarType::Number);
        assert!(pattern.matches("42 + 13 = result").unwrap());
    }

    #[test]
    fn test_constraint_pass() {
        let pattern = Pattern::new("time: {{ n }}s")
            .var("n", VarType::Number)
            .constraint("n > 0")
            .constraint("n < 1");
        assert!(pattern.matches("time: 0.5s").unwrap());
    }

    #[test]
    fn test_constraint_fail() {
        let pattern = Pattern::new("time: {{ n }}s")
            .var("n", VarType::Number)
            .constraint("n < 0");
        assert!(matches!(
            pattern.matches("time: 0.5s"),
            Err(MatchError::ConstraintNotSatisfied { .. })
        ));
    }

    #[test]
    fn test_string_match() {
        let pattern = Pattern::new("Error: {{ msg }}").var("msg", VarType::String);
        assert!(pattern.matches("Error: file not found").unwrap());
    }

    #[test]
    fn test_string_constraint() {
        let pattern = Pattern::new("Error: {{ msg }}")
            .var("msg", VarType::String)
            .constraint(r#"msg contains "not found""#);
        assert!(pattern.matches("Error: file not found").unwrap());
    }

    #[test]
    fn test_no_match() {
        let pattern = Pattern::new("passed in {{ n }}s").var("n", VarType::Number);
        assert!(!pattern.matches("failed in 0.05s").unwrap());
    }

    #[test]
    fn test_multiline() {
        let pattern = Pattern::new("line1\n{{ n }} tests\nline3").var("n", VarType::Number);
        assert!(pattern.matches("line1\n42 tests\nline3").unwrap());
    }

    #[test]
    fn test_regex_escaping() {
        let pattern = Pattern::new("test ({{ n }})").var("n", VarType::Number);
        assert!(pattern.matches("test (42)").unwrap());
    }

    #[test]
    fn test_extract_values() {
        let pattern = Pattern::new("{{ count }} items in {{ time }}s")
            .var("count", VarType::Number)
            .var("time", VarType::Number);

        match pattern.match_extract("42 items in 1.5s").unwrap() {
            MatchResult::Match(values) => {
                assert_eq!(values.get("count"), Some(&Value::Number(42.0)));
                assert_eq!(values.get("time"), Some(&Value::Number(1.5)));
            }
            MatchResult::NoMatch => panic!("expected match"),
        }
    }
}
