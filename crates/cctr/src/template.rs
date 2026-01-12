use regex::Regex;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Default)]
pub struct TemplateVars {
    vars: HashMap<String, String>,
}

impl TemplateVars {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.vars.insert(key.into(), value.into());
    }

    pub fn apply(&self, text: &str) -> String {
        let re = Regex::new(r"\{\{\s*(\w+)\s*\}\}").unwrap();
        re.replace_all(text, |caps: &regex::Captures| {
            let var_name = &caps[1];
            if let Some(value) = self.vars.get(var_name) {
                value.clone()
            } else if let Ok(value) = std::env::var(var_name) {
                value
            } else {
                caps[0].to_string()
            }
        })
        .into_owned()
    }

    pub fn apply_except(&self, text: &str, exclude: &[&str]) -> String {
        let exclude_set: HashSet<&str> = exclude.iter().copied().collect();
        let re = Regex::new(r"\{\{\s*(\w+)\s*\}\}").unwrap();
        re.replace_all(text, |caps: &regex::Captures| {
            let var_name = &caps[1];
            if exclude_set.contains(var_name) {
                caps[0].to_string()
            } else if let Some(value) = self.vars.get(var_name) {
                value.clone()
            } else if let Ok(value) = std::env::var(var_name) {
                value
            } else {
                caps[0].to_string()
            }
        })
        .into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_template_substitution() {
        let mut vars = TemplateVars::new();
        vars.set("FIXTURE_DIR", "/tmp/fixture");
        vars.set("WORK_DIR", "/tmp/work");

        let input = "cd {{ FIXTURE_DIR }} && ls {{ WORK_DIR }}";
        let output = vars.apply(input);
        assert_eq!(output, "cd /tmp/fixture && ls /tmp/work");
    }

    #[test]
    fn test_no_substitution() {
        let vars = TemplateVars::new();
        let input = "echo hello";
        let output = vars.apply(input);
        assert_eq!(output, "echo hello");
    }

    #[test]
    fn test_multiple_same_var() {
        let mut vars = TemplateVars::new();
        vars.set("DIR", "/home");

        let input = "cd {{ DIR }} && ls {{ DIR }}";
        let output = vars.apply(input);
        assert_eq!(output, "cd /home && ls /home");
    }

    #[test]
    fn test_env_var_substitution() {
        std::env::set_var("CCTR_TEST_VAR", "test_value");
        let vars = TemplateVars::new();
        let input = "value={{ CCTR_TEST_VAR }}";
        let output = vars.apply(input);
        assert_eq!(output, "value=test_value");
        std::env::remove_var("CCTR_TEST_VAR");
    }

    #[test]
    fn test_explicit_var_overrides_env() {
        std::env::set_var("CCTR_TEST_VAR2", "env_value");
        let mut vars = TemplateVars::new();
        vars.set("CCTR_TEST_VAR2", "explicit_value");
        let input = "value={{ CCTR_TEST_VAR2 }}";
        let output = vars.apply(input);
        assert_eq!(output, "value=explicit_value");
        std::env::remove_var("CCTR_TEST_VAR2");
    }

    #[test]
    fn test_unknown_var_unchanged() {
        let vars = TemplateVars::new();
        let input = "value={{ NONEXISTENT_VAR_12345 }}";
        let output = vars.apply(input);
        assert_eq!(output, "value={{ NONEXISTENT_VAR_12345 }}");
    }
}
