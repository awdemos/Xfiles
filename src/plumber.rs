use crate::message::Message;
use regex::Regex;
use std::collections::HashMap;
use std::sync::Arc;

/// Content-based routing rule inspired by Plan 9 plumber.
#[derive(Debug, Clone)]
pub struct PlumberRule {
    pub name: String,
    pub pattern: Regex,
    pub destination: String,
    pub priority: i32,
    pub header_match: Option<HashMap<String, String>>,
}

/// The plumber routes messages based on content patterns.
#[derive(Debug, Clone, Default)]
pub struct Plumber {
    rules: Arc<parking_lot::RwLock<Vec<PlumberRule>>>,
}

impl Plumber {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_rule(
        &self,
        name: &str,
        pattern: &str,
        destination: &str,
        priority: i32,
        header_match: Option<HashMap<String, String>>,
    ) -> anyhow::Result<()> {
        let re = Regex::new(pattern)?;
        let mut rules = self.rules.write();
        rules.push(PlumberRule {
            name: name.into(),
            pattern: re,
            destination: destination.into(),
            priority,
            header_match,
        });
        // Sort by priority descending
        rules.sort_by(|a, b| b.priority.cmp(&a.priority));
        Ok(())
    }

    pub fn route(&self, msg: &Message) -> Vec<String> {
        let rules = self.rules.read();
        let mut destinations = Vec::new();

        for rule in rules.iter() {
            if self.matches(rule, msg) {
                destinations.push(rule.destination.clone());
            }
        }

        if destinations.is_empty() {
            // Default fallback
            destinations.push("/proc/log".into());
        }

        destinations
    }

    fn matches(&self, rule: &PlumberRule, msg: &Message) -> bool {
        // Match against a canonical string representation of the message
        let haystack = format!(
            "type:{} sender:{} path:{} data:{}",
            msg.msg_type,
            msg.sender,
            msg.path,
            msg.data.to_string()
        );
        if !rule.pattern.is_match(&haystack) {
            return false;
        }

        // Check header constraints if present
        if let Some(ref headers) = rule.header_match {
            for (key, value) in headers {
                match msg.headers.get(key) {
                    Some(header_value) if header_value == value => continue,
                    _ => return false,
                }
            }
        }

        true
    }

    pub fn load_from_config(&self, rules: &[crate::config::PlumberRule]) -> anyhow::Result<()> {
        for r in rules {
            self.add_rule(&r.name, &r.pattern, &r.destination, r.priority, r.header_match.clone())?;
        }
        Ok(())
    }

    pub fn list_rules(&self) -> Vec<(String, String, i32)> {
        let rules = self.rules.read();
        rules
            .iter()
            .map(|r| (r.name.clone(), r.destination.clone(), r.priority))
            .collect()
    }
}
