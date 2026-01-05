use crate::expr::{eval_bool, EvalError, Value};
use crate::parse::{VarType, VariableDecl};
use regex::Regex;
use std::collections::HashMap;

#[derive(Debug)]
pub enum MatchError {
    RegexBuild(regex::Error),
    PatternMismatch {
        expected_pattern: String,
        actual: String,
    },
    ConstraintFailed {
        constraint: String,
        error: EvalError,
    },
    ConstraintNotSatisfied {
        constraint: String,
        vars: HashMap<String, Value>,
    },
}

impl std::fmt::Display for MatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MatchError::RegexBuild(e) => write!(f, "failed to build pattern regex: {}", e),
            MatchError::PatternMismatch { .. } => {
                write!(f, "output did not match expected pattern")
            }
            MatchError::ConstraintFailed { constraint, error } => {
                write!(
                    f,
                    "constraint '{}' failed to evaluate: {}",
                    constraint, error
                )
            }
            MatchError::ConstraintNotSatisfied { constraint, vars } => {
                write!(
                    f,
                    "constraint '{}' not satisfied (vars: {:?})",
                    constraint, vars
                )
            }
        }
    }
}

impl std::error::Error for MatchError {}

pub struct Matcher {
    variables: Vec<VariableDecl>,
    constraints: Vec<String>,
}

impl Matcher {
    pub fn new(variables: &[VariableDecl], constraints: &[String]) -> Self {
        Self {
            variables: variables.to_vec(),
            constraints: constraints.to_vec(),
        }
    }

    pub fn matches(&self, expected_pattern: &str, actual: &str) -> Result<bool, MatchError> {
        let regex = self.build_regex(expected_pattern)?;

        let Some(caps) = regex.captures(actual) else {
            return Ok(false);
        };

        let captured = self.extract_values(&caps)?;

        for constraint in &self.constraints {
            match eval_bool(constraint, &captured) {
                Ok(true) => {}
                Ok(false) => {
                    return Err(MatchError::ConstraintNotSatisfied {
                        constraint: constraint.clone(),
                        vars: captured,
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

        Ok(true)
    }

    fn build_regex(&self, pattern: &str) -> Result<Regex, MatchError> {
        let var_pattern = Regex::new(r"\{\{\s*(\w+)\s*\}\}").unwrap();

        let mut regex_str = String::new();
        let mut last_end = 0;

        for cap in var_pattern.captures_iter(pattern) {
            let full_match = cap.get(0).unwrap();
            let var_name = cap.get(1).unwrap().as_str();

            // Escape the literal text before this variable
            let literal = &pattern[last_end..full_match.start()];
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
                    &pattern[full_match.start()..full_match.end()],
                ));
            }

            last_end = full_match.end();
        }

        // Add remaining literal text
        regex_str.push_str(&regex::escape(&pattern[last_end..]));

        // Use dotall mode for multiline matching
        let regex_str = format!("(?s)^{}$", regex_str);

        Regex::new(&regex_str).map_err(MatchError::RegexBuild)
    }

    fn extract_values(&self, caps: &regex::Captures) -> Result<HashMap<String, Value>, MatchError> {
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

        Ok(values)
    }

    pub fn variable_names(&self) -> Vec<&str> {
        self.variables.iter().map(|v| v.name.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn num_var(name: &str) -> VariableDecl {
        VariableDecl {
            name: name.to_string(),
            var_type: VarType::Number,
        }
    }

    fn str_var(name: &str) -> VariableDecl {
        VariableDecl {
            name: name.to_string(),
            var_type: VarType::String,
        }
    }

    #[test]
    fn test_simple_number_match() {
        let matcher = Matcher::new(&[num_var("n")], &[]);
        let result = matcher.matches("passed in {{ n }}s", "passed in 0.05s");
        assert!(result.unwrap());
    }

    #[test]
    fn test_multiple_numbers() {
        let matcher = Matcher::new(&[num_var("a"), num_var("b")], &[]);
        let result = matcher.matches("{{ a }} + {{ b }} = result", "42 + 13 = result");
        assert!(result.unwrap());
    }

    #[test]
    fn test_constraint_pass() {
        let matcher = Matcher::new(&[num_var("n")], &["n > 0".to_string(), "n < 1".to_string()]);
        let result = matcher.matches("time: {{ n }}s", "time: 0.5s");
        assert!(result.unwrap());
    }

    #[test]
    fn test_constraint_fail() {
        let matcher = Matcher::new(&[num_var("n")], &["n < 0".to_string()]);
        let result = matcher.matches("time: {{ n }}s", "time: 0.5s");
        assert!(matches!(
            result,
            Err(MatchError::ConstraintNotSatisfied { .. })
        ));
    }

    #[test]
    fn test_string_match() {
        let matcher = Matcher::new(&[str_var("msg")], &[]);
        let result = matcher.matches("Error: {{ msg }}", "Error: file not found");
        assert!(result.unwrap());
    }

    #[test]
    fn test_string_constraint() {
        let matcher = Matcher::new(
            &[str_var("msg")],
            &[r#"msg contains "not found""#.to_string()],
        );
        let result = matcher.matches("Error: {{ msg }}", "Error: file not found");
        assert!(result.unwrap());
    }

    #[test]
    fn test_no_match() {
        let matcher = Matcher::new(&[num_var("n")], &[]);
        let result = matcher.matches("passed in {{ n }}s", "failed in 0.05s");
        assert!(!result.unwrap());
    }

    #[test]
    fn test_multiline() {
        let matcher = Matcher::new(&[num_var("n")], &[]);
        let result = matcher.matches("line1\n{{ n }} tests\nline3", "line1\n42 tests\nline3");
        assert!(result.unwrap());
    }

    #[test]
    fn test_regex_escaping() {
        let matcher = Matcher::new(&[num_var("n")], &[]);
        let result = matcher.matches("test ({{ n }})", "test (42)");
        assert!(result.unwrap());
    }
}
