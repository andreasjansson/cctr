//! Pattern matching for test output with variable extraction and constraints.

use crate::{VarType, VariableDecl};
use cctr_expr::{evaluate, free_variables, parse, Expr, Value};
use regex::Regex;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Error, Debug, Clone)]
pub enum MatchError {
    #[error("failed to build pattern regex: {0}")]
    RegexBuild(#[from] regex::Error),
    #[error("duplicate variable '{{{{ {0} }}}}' in pattern - each variable can only appear once")]
    DuplicateVariable(String),
    #[error("{0}")]
    Constraint(Box<ConstraintFailure>),
    #[error("failed to parse JSON for variable '{name}': {error}")]
    JsonParse { name: String, error: String },
}

/// A `where` constraint that did not hold, with the context needed to see why.
#[derive(Debug, Clone, PartialEq)]
pub struct ConstraintFailure {
    pub constraint: String,
    /// `None` when the constraint evaluated to false; the error message when
    /// evaluating it failed outright.
    pub error: Option<String>,
    /// Intermediate values that explain a constraint that evaluated to false.
    pub details: Vec<Detail>,
    /// Values of the variables the constraint references, sorted by name.
    pub bindings: Vec<(String, Value)>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Detail {
    /// A non-trivial operand of a failing comparison and what it evaluated to.
    Operand { expr: String, value: Value },
    /// The first element for which a `forall` predicate was false.
    Counterexample {
        var: String,
        value: Value,
        location: String,
    },
}

impl std::fmt::Display for ConstraintFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.error {
            None => write!(f, "constraint '{}' not satisfied", self.constraint),
            Some(error) => write!(f, "constraint '{}' failed: {}", self.constraint, error),
        }
    }
}

/// Evaluate one constraint, returning why it failed if it did not hold.
fn check_constraint(constraint: &str, vars: &HashMap<String, Value>) -> Option<ConstraintFailure> {
    let expr = match parse(constraint) {
        Ok(expr) => expr,
        Err(e) => {
            return Some(ConstraintFailure {
                constraint: constraint.to_string(),
                error: Some(e.to_string()),
                details: Vec::new(),
                bindings: Vec::new(),
            })
        }
    };

    let error = match evaluate(&expr, vars).and_then(|v| v.as_bool()) {
        Ok(true) => return None,
        Ok(false) => None,
        Err(e) => Some(e.to_string()),
    };

    let details = if error.is_none() {
        explain(&expr, vars)
    } else {
        Vec::new()
    };

    let bindings = free_variables(&expr)
        .into_iter()
        .filter_map(|name| vars.get(&name).map(|v| (name, v.clone())))
        .collect();

    Some(ConstraintFailure {
        constraint: constraint.to_string(),
        error,
        details,
        bindings,
    })
}

/// Intermediate values that show why a constraint evaluated to false.
///
/// For a comparison, the value of each operand that is neither a literal nor a
/// bare variable (bare variables already appear among the bindings). For a
/// `forall`, the first element the predicate rejected.
fn explain(expr: &Expr, vars: &HashMap<String, Value>) -> Vec<Detail> {
    match expr {
        Expr::BinaryOp { op, left, right } if op.is_comparison() => [left, right]
            .into_iter()
            .filter(|side| !side.is_literal() && !matches!(side.as_ref(), Expr::Var(_)))
            .filter_map(|side| {
                evaluate(side, vars).ok().map(|value| Detail::Operand {
                    expr: side.to_string(),
                    value,
                })
            })
            .collect(),
        Expr::ForAll {
            predicate,
            var,
            iterable,
        } => counterexample(predicate, var, iterable, vars)
            .into_iter()
            .collect(),
        _ => Vec::new(),
    }
}

fn counterexample(
    predicate: &Expr,
    var: &str,
    iterable: &Expr,
    vars: &HashMap<String, Value>,
) -> Option<Detail> {
    let base = match iterable {
        Expr::BinaryOp { .. } | Expr::UnaryOp { .. } | Expr::ForAll { .. } => {
            format!("({})", iterable)
        }
        _ => iterable.to_string(),
    };
    let items: Vec<(String, Value)> = match evaluate(iterable, vars).ok()? {
        Value::Array(items) => items
            .into_iter()
            .enumerate()
            .map(|(i, v)| (format!("{}[{}]", base, i), v))
            .collect(),
        Value::Object(obj) => {
            let mut entries: Vec<_> = obj.into_iter().collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            entries
                .into_iter()
                .map(|(k, v)| (format!("{}[{:?}]", base, k), v))
                .collect()
        }
        _ => return None,
    };

    let mut local = vars.clone();
    for (location, value) in items {
        local.insert(var.to_string(), value.clone());
        if let Ok(Value::Bool(false)) = evaluate(predicate, &local) {
            return Some(Detail::Counterexample {
                var: var.to_string(),
                value,
                location,
            });
        }
    }
    None
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

        for constraint in self.constraints {
            if let Some(failure) = check_constraint(constraint, &all_values) {
                return Err(MatchError::Constraint(Box::new(failure)));
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

    fn no_prior() -> HashMap<String, Value> {
        HashMap::new()
    }

    #[test]
    fn test_simple_number_match() {
        let vars = vec![make_var("n", Some("number"))];
        let constraints = vec![];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(
            matcher
                .matches("passed in {{ n }}s", "passed in 0.05s", &no_prior())
                .unwrap()
                .matched
        );
    }

    #[test]
    fn test_constraint_pass() {
        let vars = vec![make_var("n", Some("number"))];
        let constraints = vec!["n > 0".to_string(), "n < 1".to_string()];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(
            matcher
                .matches("time: {{ n }}s", "time: 0.5s", &no_prior())
                .unwrap()
                .matched
        );
    }

    #[test]
    fn test_constraint_fail() {
        let vars = vec![make_var("n", Some("number"))];
        let constraints = vec!["n < 0".to_string()];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        let result = matcher.matches("time: {{ n }}s", "time: 0.5s", &no_prior());
        let Err(MatchError::Constraint(failure)) = result else {
            panic!("expected a constraint failure");
        };
        assert_eq!(failure.constraint, "n < 0");
        assert_eq!(failure.error, None);
        assert_eq!(
            failure.bindings,
            vec![("n".to_string(), Value::Number(0.5))]
        );
    }

    fn failure_for(constraint: &str, vars: &[(&str, Value)]) -> ConstraintFailure {
        let vars: HashMap<String, Value> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect();
        check_constraint(constraint, &vars).expect("constraint should fail")
    }

    fn json(text: &str) -> Value {
        json_to_value(&serde_json::from_str(text).unwrap()).unwrap()
    }

    #[test]
    fn test_passing_constraint_is_not_a_failure() {
        let vars = HashMap::from([("n".to_string(), Value::Number(1.0))]);
        assert_eq!(check_constraint("n == 1", &vars), None);
    }

    #[test]
    fn test_evaluation_error_carries_bindings() {
        // Previously only false constraints reported bindings; an evaluation
        // error reported none, which is exactly when you most need them.
        let entries = json(r#"[{"messages": [{"details": {"status": "ok"}}]}, {"x": 1}]"#);
        let failure = failure_for(
            "entries[-2].messages[0].details.result == 8",
            &[("entries", entries.clone()), ("unrelated", Value::Null)],
        );
        assert_eq!(
            failure.error.as_deref(),
            Some(
                r#"key "result" not found in entries[-2].messages[0].details (available keys: "status")"#
            )
        );
        assert_eq!(failure.bindings, vec![("entries".to_string(), entries)]);
        assert!(failure.details.is_empty());
    }

    #[test]
    fn test_bindings_are_only_the_referenced_variables() {
        let failure = failure_for(
            "t < 1",
            &[("t", Value::Number(2.0)), ("n", Value::Number(42.0))],
        );
        assert_eq!(
            failure.bindings,
            vec![("t".to_string(), Value::Number(2.0))]
        );
    }

    #[test]
    fn test_forall_bound_variable_is_not_a_binding() {
        let failure = failure_for(
            "x < limit forall x in items",
            &[
                ("items", json("[1, 2, 10]")),
                ("limit", Value::Number(5.0)),
                ("x", Value::Number(-1.0)),
            ],
        );
        let names: Vec<&str> = failure.bindings.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["items", "limit"]);
    }

    #[test]
    fn test_comparison_reports_non_trivial_operands() {
        let failure = failure_for(
            r#"o.users[0].name == "charlie""#,
            &[("o", json(r#"{"users": [{"name": "alice"}]}"#))],
        );
        assert_eq!(
            failure.details,
            vec![Detail::Operand {
                expr: "o.users[0].name".to_string(),
                value: Value::String("alice".to_string()),
            }]
        );
    }

    #[test]
    fn test_comparison_reports_both_operands_when_neither_is_trivial() {
        let failure = failure_for(
            "len(a) == len(b)",
            &[("a", json("[1, 2]")), ("b", json("[1]"))],
        );
        assert_eq!(
            failure.details,
            vec![
                Detail::Operand {
                    expr: "len(a)".to_string(),
                    value: Value::Number(2.0),
                },
                Detail::Operand {
                    expr: "len(b)".to_string(),
                    value: Value::Number(1.0),
                },
            ]
        );
    }

    #[test]
    fn test_bare_variables_and_literals_are_not_repeated_as_details() {
        let failure = failure_for("n == 999", &[("n", Value::Number(7.0))]);
        assert!(failure.details.is_empty());
    }

    #[test]
    fn test_forall_reports_first_counterexample() {
        let failure = failure_for("x < 5 forall x in a", &[("a", json("[1, 2, 10, 20]"))]);
        assert_eq!(
            failure.details,
            vec![Detail::Counterexample {
                var: "x".to_string(),
                value: Value::Number(10.0),
                location: "a[2]".to_string(),
            }]
        );
    }

    #[test]
    fn test_forall_over_object_reports_key_deterministically() {
        let failure = failure_for(
            "type(v) == number forall v in o",
            &[("o", json(r#"{"c": "x", "a": 1, "b": "y"}"#))],
        );
        assert_eq!(
            failure.details,
            vec![Detail::Counterexample {
                var: "v".to_string(),
                value: Value::String("y".to_string()),
                location: r#"o["b"]"#.to_string(),
            }]
        );
    }

    #[test]
    fn test_unparseable_constraint_is_a_failure_with_error() {
        let failure = failure_for("n >", &[("n", Value::Number(1.0))]);
        assert!(failure.error.is_some());
    }

    #[test]
    fn test_no_match() {
        let vars = vec![make_var("n", Some("number"))];
        let constraints = vec![];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(
            !matcher
                .matches("passed in {{ n }}s", "failed in 0.05s", &no_prior())
                .unwrap()
                .matched
        );
    }

    #[test]
    fn test_empty_string_match() {
        let vars = vec![make_var("s", Some("string"))];
        let constraints = vec!["len(s) == 0".to_string()];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(
            matcher
                .matches("val: {{ s }}", "val: ", &no_prior())
                .unwrap()
                .matched
        );
    }

    #[test]
    fn test_json_string_match() {
        let vars = vec![make_var("s", Some("json string"))];
        let constraints = vec![r#"s == "hello""#.to_string()];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(
            matcher
                .matches("{{ s }}", r#""hello""#, &no_prior())
                .unwrap()
                .matched
        );
    }

    #[test]
    fn test_json_string_length() {
        let vars = vec![make_var("s", Some("json string"))];
        let constraints = vec!["len(s) == 5".to_string()];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(
            matcher
                .matches("{{ s }}", r#""hello""#, &no_prior())
                .unwrap()
                .matched
        );
    }

    #[test]
    fn test_json_bool_true() {
        let vars = vec![make_var("b", Some("json bool"))];
        let constraints = vec!["b == true".to_string()];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(
            matcher
                .matches("{{ b }}", "true", &no_prior())
                .unwrap()
                .matched
        );
    }

    #[test]
    fn test_json_bool_false() {
        let vars = vec![make_var("b", Some("json bool"))];
        let constraints = vec!["b == false".to_string()];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(
            matcher
                .matches("{{ b }}", "false", &no_prior())
                .unwrap()
                .matched
        );
    }

    #[test]
    fn test_json_array_match() {
        let vars = vec![make_var("a", Some("json array"))];
        let constraints = vec!["len(a) == 3".to_string(), "a[0] == 1".to_string()];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(
            matcher
                .matches("{{ a }}", "[1, 2, 3]", &no_prior())
                .unwrap()
                .matched
        );
    }

    #[test]
    fn test_json_object_match() {
        let vars = vec![make_var("o", Some("json object"))];
        let constraints = vec![r#"o["name"] == "alice""#.to_string()];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(
            matcher
                .matches("{{ o }}", r#"{"name": "alice", "age": 30}"#, &no_prior())
                .unwrap()
                .matched
        );
    }

    #[test]
    fn test_json_object_dot_access() {
        let vars = vec![make_var("o", Some("json object"))];
        let constraints = vec!["o.age == 30".to_string()];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(
            matcher
                .matches("{{ o }}", r#"{"name": "alice", "age": 30}"#, &no_prior())
                .unwrap()
                .matched
        );
    }

    #[test]
    fn test_json_forall() {
        let vars = vec![make_var("a", Some("json array"))];
        let constraints = vec!["x <= 3 forall x in a".to_string()];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(
            matcher
                .matches("{{ a }}", "[1, 2, 3]", &no_prior())
                .unwrap()
                .matched
        );
    }

    #[test]
    fn test_duck_typed_number() {
        let vars = vec![make_var("x", None)];
        let constraints = vec!["x > 0".to_string()];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(
            matcher
                .matches("val: {{ x }}", "val: 42", &no_prior())
                .unwrap()
                .matched
        );
    }

    #[test]
    fn test_duck_typed_string() {
        let vars = vec![make_var("x", None)];
        let constraints = vec!["len(x) == 5".to_string()];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(
            matcher
                .matches("val: {{ x }}", "val: hello", &no_prior())
                .unwrap()
                .matched
        );
    }

    #[test]
    fn test_duck_typed_bool() {
        let vars = vec![make_var("x", None)];
        let constraints = vec!["x == true".to_string()];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(
            matcher
                .matches("val: {{ x }}", "val: true", &no_prior())
                .unwrap()
                .matched
        );
    }

    #[test]
    fn test_inline_type_annotation_stripped() {
        let vars = vec![make_var("n", Some("number"))];
        let constraints = vec![];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        assert!(
            matcher
                .matches("val: {{ n: number }}", "val: 42", &no_prior())
                .unwrap()
                .matched
        );
    }

    #[test]
    fn test_prior_vars_available_in_constraints() {
        let vars = vec![make_var("b", Some("number"))];
        let constraints = vec!["b == a + 1".to_string()];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        let mut prior = HashMap::new();
        prior.insert("a".to_string(), Value::Number(41.0));

        assert!(matcher.matches("{{ b }}", "42", &prior).unwrap().matched);
    }

    #[test]
    fn test_prior_vars_overridden_by_new_capture() {
        let vars = vec![make_var("x", Some("number"))];
        let constraints = vec!["x == 99".to_string()];
        let matcher = Matcher::new(&vars, &constraints, &[]);

        let mut prior = HashMap::new();
        prior.insert("x".to_string(), Value::Number(1.0));

        let result = matcher.matches("{{ x }}", "99", &prior).unwrap();
        assert!(result.matched);
        assert_eq!(result.captured.get("x"), Some(&Value::Number(99.0)));
    }
}
