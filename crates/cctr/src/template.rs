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
        let mut result = text.to_string();
        for (key, value) in &self.vars {
            let placeholder = format!("{{{{ {} }}}}", key);
            result = result.replace(&placeholder, value);
        }
        result
    }

    pub fn apply_except(&self, text: &str, exclude: &[&str]) -> String {
        let exclude_set: HashSet<&str> = exclude.iter().copied().collect();
        let mut result = text.to_string();
        for (key, value) in &self.vars {
            if !exclude_set.contains(key.as_str()) {
                let placeholder = format!("{{{{ {} }}}}", key);
                result = result.replace(&placeholder, value);
            }
        }
        result
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
}
