//! `config/sidekiq.yml`: the worker's defaults, read at compile time.
//!
//! Sidekiq 8.1's CLI (`Sidekiq::CLI#parse_config`) runs the file through
//! ERB, loads the YAML, symbolizes its keys and merges the current
//! environment's section over the top level:
//!
//! ```ruby
//! opts = YAML.safe_load(erb.result, permitted_classes: [Symbol], aliases: true) || {}
//! opts.deep_symbolize_keys!
//! opts = opts.merge(opts.delete(environment.to_sym) || {})
//! ```
//!
//! The command line then wins key by key, and `-q` replaces the file's
//! queues whole. The emitted worker keeps that order: its `-c`, `-q` and
//! `-t` win over what this reads, and this wins over `RAILS_MAX_THREADS`.
//!
//! An emitted tree runs in production, so the `production:` section is
//! the one merged. Only `concurrency`, `timeout` and `queues` are read,
//! with or without the leading colon. A queue is a name, `"name,weight"`,
//! or `[name, weight]`, as `Capsule#queues=` reads them. A value that is
//! ERB, a key the worker has no use for (`capsules`, `max_retries`,
//! `logfile`, ...), and a value in another shape are each a warning and
//! are left to the command line.

use std::path::Path;

use serde_yaml_ng::Value;

use crate::app::SidekiqWorkerDefaults;
use crate::vfs::Vfs;

/// Stands in for an ERB tag while the YAML is read, so a value that was
/// ERB can be named in its warning rather than silently read as `""`.
const ERB: &str = "__roundhouse_erb__";

/// The defaults `config/sidekiq.yml` gives, and a warning for each part
/// of it that isn't read. Empty when the file is absent.
pub fn read<V: Vfs + ?Sized>(vfs: &V, dir: &Path) -> (SidekiqWorkerDefaults, Vec<String>) {
    let Ok(bytes) = vfs.read(&dir.join("config/sidekiq.yml")) else { return Default::default() };
    parse(&String::from_utf8_lossy(&bytes))
}

pub fn parse(text: &str) -> (SidekiqWorkerDefaults, Vec<String>) {
    let mut defaults = SidekiqWorkerDefaults::default();
    let mut warnings = Vec::new();
    let (text, logic) = mark_erb(text);
    if logic {
        warnings.push("config/sidekiq.yml has ERB that isn't an output tag (`<% ... %>`); it is not run".to_string());
    }
    let mut doc = match serde_yaml_ng::from_str::<Value>(&text) {
        Ok(doc) => doc,
        Err(e) => {
            warnings.push(format!("config/sidekiq.yml is not read: {e}"));
            return (defaults, warnings);
        }
    };
    if doc.apply_merge().is_err() {
        warnings.push("config/sidekiq.yml's merge keys don't resolve; it is not read".to_string());
        return (defaults, warnings);
    }
    let Value::Mapping(top) = doc else { return (defaults, warnings) };
    let mut entries: Vec<(String, Value)> = Vec::new();
    let mut production = None;
    for (key, value) in top {
        let Some(key) = key_name(&key) else { continue };
        if key == "production" {
            production = Some(value);
        } else if matches!(value, Value::Mapping(_)) && key != "capsules" {
            // Another environment's section.
        } else {
            entries.push((key, value));
        }
    }
    if let Some(Value::Mapping(section)) = production {
        for (key, value) in section {
            let Some(key) = key_name(&key) else { continue };
            entries.retain(|(k, _)| k != &key);
            entries.push((key, value));
        }
    }
    for (key, value) in entries {
        if mentions_erb(&value) {
            warnings.push(format!(
                "`:{key}` in config/sidekiq.yml is ERB, so it is not read; give it on the worker's command line"
            ));
            continue;
        }
        match key.as_str() {
            "concurrency" => match value.as_i64() {
                Some(n) if n > 0 => defaults.concurrency = Some(n),
                _ => warnings.push(format!("`:concurrency: {}` in config/sidekiq.yml is not a positive Integer, so it is not read", show(&value))),
            },
            "timeout" => match value.as_i64() {
                Some(n) if n >= 0 => defaults.timeout = Some(n),
                _ => warnings.push(format!("`:timeout: {}` in config/sidekiq.yml is not an Integer, so it is not read", show(&value))),
            },
            "queues" => match queues(&value) {
                Some(q) => defaults.queues = q,
                None => warnings.push(format!(
                    "`:queues:` in config/sidekiq.yml is not a list of names, `name,weight` or `[name, weight]`, so it is not read"
                )),
            },
            "strict" => {}
            other => warnings.push(format!("`:{other}` in config/sidekiq.yml is not read by the Roundhouse worker")),
        }
    }
    (defaults, warnings)
}

/// `concurrency`, `:concurrency` and the symbol `:concurrency` alike.
fn key_name(key: &Value) -> Option<String> {
    let Value::String(s) = key else { return None };
    Some(s.strip_prefix(':').unwrap_or(s).to_string())
}

fn queues(value: &Value) -> Option<Vec<(String, i64)>> {
    let list = match value {
        Value::Sequence(list) => list.clone(),
        Value::String(_) => vec![value.clone()],
        _ => return None,
    };
    let mut out = Vec::new();
    for item in list {
        match item {
            Value::String(s) => {
                let mut parts = s.splitn(2, ',');
                let name = parts.next()?.trim().to_string();
                let weight = match parts.next() {
                    Some(w) => w.trim().parse::<i64>().ok()?,
                    None => 0,
                };
                if name.is_empty() {
                    return None;
                }
                out.push((name, weight));
            }
            Value::Sequence(pair) => {
                let name = match pair.first()? {
                    Value::String(s) => s.clone(),
                    _ => return None,
                };
                let weight = match pair.get(1) {
                    None => 0,
                    Some(w) => w.as_i64()?,
                };
                out.push((name, weight));
            }
            _ => return None,
        }
    }
    if out.is_empty() { None } else { Some(out) }
}

fn mentions_erb(value: &Value) -> bool {
    match value {
        Value::String(s) => s.contains(ERB),
        Value::Sequence(list) => list.iter().any(mentions_erb),
        Value::Mapping(map) => map.iter().any(|(k, v)| mentions_erb(k) || mentions_erb(v)),
        _ => false,
    }
}

fn show(value: &Value) -> String {
    serde_yaml_ng::to_string(value).map(|s| s.trim().to_string()).unwrap_or_default()
}

/// Each `<%= ... %>` becomes the ERB marker, and each other tag
/// nothing. The second answer says whether there were any of those.
fn mark_erb(text: &str) -> (String, bool) {
    let mut out = String::with_capacity(text.len());
    let mut logic = false;
    let mut rest = text;
    while let Some(start) = rest.find("<%") {
        out.push_str(&rest[..start]);
        let tag = &rest[start..];
        match tag.find("%>") {
            Some(end) => {
                if tag.starts_with("<%=") {
                    out.push_str(ERB);
                } else if !tag.starts_with("<%#") {
                    logic = true;
                }
                rest = &tag[end + 2..];
            }
            None => rest = "",
        }
    }
    out.push_str(rest);
    (out, logic)
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn symbol_keys_and_the_production_section_merge_over_the_top_level() {
        let (d, w) = parse(
            ":concurrency: 5\n:timeout: 8\n:queues:\n  - critical\n  - [default, 2]\n  - low,1\nproduction:\n  :concurrency: 10\ndevelopment:\n  :concurrency: 1\n",
        );
        assert!(w.is_empty(), "{w:#?}");
        assert_eq!(d.concurrency, Some(10));
        assert_eq!(d.timeout, Some(8));
        assert_eq!(d.queues, vec![("critical".into(), 0), ("default".into(), 2), ("low".into(), 1)]);
    }

    #[test]
    fn plain_keys_read_too() {
        let (d, w) = parse("concurrency: 3\nqueues: [default]\n");
        assert!(w.is_empty(), "{w:#?}");
        assert_eq!(d.concurrency, Some(3));
        assert_eq!(d.queues, vec![("default".into(), 0)]);
    }

    #[test]
    fn erb_and_unknown_keys_warn_and_are_not_read() {
        let (d, w) = parse(
            ":concurrency: <%= ENV.fetch(\"SIDEKIQ_CONCURRENCY\", 5) %>\n:max_retries: 3\n:queues:\n  - default\n",
        );
        assert_eq!(d.concurrency, None);
        assert_eq!(d.queues, vec![("default".into(), 0)]);
        assert_eq!(w.len(), 2, "{w:#?}");
        assert!(w[0].contains(":concurrency") && w[0].contains("ERB"), "{w:#?}");
        assert!(w[1].contains(":max_retries"), "{w:#?}");
    }

    #[test]
    fn a_bad_shape_warns() {
        let (d, w) = parse(":concurrency: lots\n:queues: 3\n");
        assert_eq!(d, Default::default());
        assert_eq!(w.len(), 2, "{w:#?}");
    }
}
