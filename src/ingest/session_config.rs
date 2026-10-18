use std::path::Path;

use ruby_prism::{Node, Visit};

use super::util::{
    constant_id_str, constant_path_of, flatten_statements, string_value, symbol_value,
};
use super::{IngestError, IngestResult};
use crate::vfs::Vfs;

fn store_call(node: &Node<'_>) -> bool {
    let Some(call) = node.as_call_node() else {
        return false;
    };
    constant_id_str(&call.name()) == "session_store"
        && call
            .receiver()
            .and_then(|r| r.as_call_node())
            .is_some_and(|r| constant_id_str(&r.name()) == "config" && r.arguments().is_none())
}

fn text(node: &Node<'_>, src: &str) -> String {
    let loc = node.location();
    src[loc.start_offset()..loc.end_offset()].to_string()
}

fn statements<'pr>(node: Option<Node<'pr>>) -> Vec<Node<'pr>> {
    node.map(|node| match node.as_program_node() {
        Some(program) => program.statements().body().iter().collect(),
        None => flatten_statements(node),
    })
    .unwrap_or_default()
}

fn read_steps(nodes: Vec<Node<'_>>, src: &str, count: &mut usize) -> Result<String, String> {
    let mut out = String::new();
    for node in nodes {
        if let Some(class) = node.as_class_node() {
            out.push_str(&read_steps(statements(class.body()), src, count)?);
        } else if let Some(module) = node.as_module_node() {
            out.push_str(&read_steps(statements(module.body()), src, count)?);
        } else if let Some(branch) = node.as_if_node() {
            let yes = read_steps(
                branch
                    .statements()
                    .map(|s| s.body().iter().collect())
                    .unwrap_or_default(),
                src,
                count,
            )?;
            let no = match branch.subsequent() {
                Some(other) => match other.as_else_node() {
                    Some(other) => read_steps(
                        other
                            .statements()
                            .map(|s| s.body().iter().collect())
                            .unwrap_or_default(),
                        src,
                        count,
                    )?,
                    None => read_steps(vec![other], src, count)?,
                },
                None => String::new(),
            };
            if !yes.is_empty() || !no.is_empty() {
                out.push_str(&format!(
                    "if {}\n{yes}else\n{no}end\n",
                    text(&branch.predicate(), src)
                ));
            }
        } else if let Some(branch) = node.as_unless_node() {
            let yes = read_steps(
                branch
                    .statements()
                    .map(|s| s.body().iter().collect())
                    .unwrap_or_default(),
                src,
                count,
            )?;
            let no = read_steps(
                branch
                    .else_clause()
                    .and_then(|s| s.statements())
                    .map(|s| s.body().iter().collect())
                    .unwrap_or_default(),
                src,
                count,
            )?;
            if !yes.is_empty() || !no.is_empty() {
                out.push_str(&format!(
                    "unless {}\n{yes}else\n{no}end\n",
                    text(&branch.predicate(), src)
                ));
            }
        } else if store_call(&node) {
            *count += 1;
            let call = node.as_call_node().unwrap();
            let args = call
                .arguments()
                .ok_or("session_store has no store argument")?;
            let mut key = "_session_id".to_string();
            for arg in args.arguments().iter() {
                let Some(kwargs) = arg.as_keyword_hash_node() else {
                    continue;
                };
                for element in kwargs.elements().iter() {
                    let pair = element
                        .as_assoc_node()
                        .ok_or("a session_store options splat")?;
                    if symbol_value(&pair.key()).as_deref() == Some("key") {
                        key = string_value(&pair.value())
                            .ok_or("session_store key is not a literal String")?;
                    }
                }
            }
            out.push_str(&format!("key = {key:?}\n"));
        } else if let Some(call) = node.as_call_node()
            && constant_id_str(&call.name()) == "configure"
        {
            let body = call
                .block()
                .and_then(|b| b.as_block_node())
                .and_then(|b| b.body());
            out.push_str(&read_steps(statements(body), src, count)?);
        }
    }
    Ok(out)
}

fn read_file<V: Vfs + ?Sized>(vfs: &V, path: &Path) -> IngestResult<String> {
    let Ok(bytes) = vfs.read(path) else {
        return Ok(String::new());
    };
    let parsed = super::prism::parse(&bytes, &path.display().to_string());
    let mut count = 0;
    let found = read_steps(
        statements(Some(parsed.node())),
        &String::from_utf8_lossy(&bytes),
        &mut count,
    );
    struct Calls(usize);
    impl<'pr> Visit<'pr> for Calls {
        fn visit_call_node(&mut self, call: &ruby_prism::CallNode<'pr>) {
            if store_call(&call.as_node()) {
                self.0 += 1;
            }
            ruby_prism::visit_call_node(self, call);
        }
    }
    let mut calls = Calls(0);
    calls.visit(&parsed.node());
    let found = found.and_then(|code| {
        if calls.0 == count {
            Ok(code)
        } else {
            Err("session_store is outside a supported config body or conditional".to_string())
        }
    });
    match found {
        Ok(code) => Ok(code),
        Err(message) => {
            super::survey::continue_or_fail(IngestError::Unsupported {
                file: path.display().to_string(),
                message,
            })?;
            Ok(String::new())
        }
    }
}

pub(super) fn reads(source: &[u8]) -> bool {
    let parsed = ruby_prism::parse(source);
    let mut count = 0;
    read_steps(
        statements(Some(parsed.node())),
        &String::from_utf8_lossy(source),
        &mut count,
    )
    .is_ok()
        && count > 0
}

fn application_name(nodes: Vec<Node<'_>>, prefix: &[String]) -> Option<String> {
    for node in nodes {
        if let Some(module) = node.as_module_node() {
            let mut path = prefix.to_vec();
            path.extend(constant_path_of(&module.constant_path())?);
            if let Some(name) = application_name(statements(module.body()), &path) {
                return Some(name);
            }
        } else if let Some(class) = node.as_class_node() {
            let mut path = prefix.to_vec();
            path.extend(constant_path_of(&class.constant_path())?);
            if class
                .superclass()
                .and_then(|n| constant_path_of(&n))
                .is_some_and(|p| p == ["Rails", "Application"])
            {
                return Some(path.join("::"));
            }
            if let Some(name) = application_name(statements(class.body()), &path) {
                return Some(name);
            }
        }
    }
    None
}

fn default_key(name: &str) -> String {
    let chars: Vec<char> = name.replace("::", "_").chars().collect();
    let mut underscored = String::new();
    for (i, c) in chars.iter().enumerate() {
        if c.is_ascii_uppercase()
            && i > 0
            && (chars[i - 1].is_ascii_lowercase()
                || chars[i - 1].is_ascii_digit()
                || chars[i - 1].is_ascii_uppercase()
                    && chars.get(i + 1).is_some_and(char::is_ascii_lowercase))
        {
            underscored.push('_');
        }
        underscored.push(c.to_ascii_lowercase());
    }
    format!(
        "_{}_session",
        underscored
            .strip_suffix("_application")
            .unwrap_or(&underscored)
    )
}

pub(super) fn synthesize<V: Vfs + ?Sized>(
    vfs: &V,
    dir: &Path,
    source: &[u8],
) -> IngestResult<Option<String>> {
    let parsed = ruby_prism::parse(source);
    let name = application_name(statements(Some(parsed.node())), &[]);
    let mut code = read_file(vfs, &dir.join("config/application.rb"))?;
    let env_dir = dir.join("config/environments");
    if vfs.is_dir(&env_dir) {
        let mut paths = vfs.read_dir(&env_dir)?;
        paths.sort();
        for path in paths
            .into_iter()
            .filter(|p| p.extension().is_some_and(|e| e == "rb"))
        {
            let body = read_file(vfs, &path)?;
            if !body.is_empty() {
                let env = path.file_stem().unwrap().to_string_lossy();
                code.push_str(&format!("if Rails.env.to_s == {env:?}\n{body}end\n"));
            }
        }
    }
    let init_dir = dir.join("config/initializers");
    if vfs.is_dir(&init_dir) {
        for path in super::app::read_rb_files(vfs, &init_dir)? {
            code.push_str(&read_file(vfs, &path)?);
        }
    }
    if name.is_none() && code.is_empty() {
        return Ok(None);
    }
    let key = name
        .as_deref()
        .map(default_key)
        .unwrap_or_else(|| "_session".to_string());
    Ok(Some(format!(
        "def session_cookie_key\nkey = {key:?}\n{code}key\nend\n"
    )))
}
