//! Strangler inventory signal: drive `runtime_loader::python_units` over
//! the framework Ruby and emit each as `app/*.py`. Keeps the new
//! transpile path exercised while it's still dormant in `emit::python`.
//!
//! The test writes the emitted files to a temp dir and (if `python3` is
//! on PATH) reports which transpile to syntactically valid Python via
//! `py_compile` — the per-file readiness signal for graduating an entry
//! from hand-written `runtime/python/*.py` to transpiled.

use roundhouse::runtime_loader::python_units;
use std::process::Command;

#[test]
fn python_framework_units_emit() {
    let units = python_units(|_path, classes| classes).expect("python_units");
    assert!(!units.is_empty(), "expected framework units");

    let dir = std::path::PathBuf::from("tmp/rh-py-framework-units");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let py = Command::new("python3").arg("--version").output().is_ok();
    let mut valid = 0;
    let mut total = 0;
    println!("\n  file                              bytes  py_compile");
    println!("  -------------------------------------------------------");
    for u in &units {
        total += 1;
        assert!(!u.content.trim().is_empty(), "{:?} emitted empty", u.out_path);
        let flat = u.out_path.to_string_lossy().replace('/', "__");
        let path = dir.join(&flat);
        std::fs::write(&path, &u.content).unwrap();
        let status = if py {
            let out = Command::new("python3")
                .arg("-m")
                .arg("py_compile")
                .arg(&path)
                .output()
                .unwrap();
            if out.status.success() {
                valid += 1;
                "OK".to_string()
            } else {
                let err = String::from_utf8_lossy(&out.stderr);
                let last = err.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("");
                format!("FAIL — {}", last.trim())
            }
        } else {
            "skipped (no python3)".to_string()
        };
        println!("  {:32} {:6} {}", u.out_path.to_string_lossy(), u.content.len(), status);
    }
    println!("\n  STRANGLER READINESS: {valid}/{total} framework files transpile to valid Python\n");
}

/// Python runs module constants at import and method bodies later, so
/// the class block is emitted first. `ControllerConfig()` must not run
/// before `class ControllerConfig`.
#[test]
fn python_controller_config_slot_follows_its_class() {
    use roundhouse::runtime_loader::python_units_subset;
    let units = python_units_subset(&["app/action_controller_base.py"], |_path, classes| classes)
        .expect("python_units_subset");
    let body = &units[0].content;
    let class_at = body.find("class ControllerConfig:").expect("ControllerConfig class");
    let slot_at = body.find("CONTROLLER_CONFIG_SLOT = ").expect("config slot");
    let last_class = body.rfind("\nclass ").expect("a class");
    assert!(
        last_class < slot_at && class_at < slot_at,
        "classes come first, then the constant block:\n{body}"
    );
    // `slot_at` points at `C`, so a slice from there hides leading
    // spaces. The line has to start at the preceding newline or the
    // assertion would pass for an indented class variable.
    let slot_line_start = body[..slot_at].rfind('\n').map_or(0, |i| i + 1);
    let slot_line = body[slot_line_start..].lines().next().unwrap_or("");
    assert!(
        !slot_line.starts_with(' '),
        "the slot stays a module constant, got {slot_line:?}"
    );
}

/// A default argument is evaluated while the class body runs, so a
/// constant it names has to be bound before `class ViewHelpers`.
/// `CONTROLLER_CONFIG_SLOT` still follows its class (the test above);
/// `EMPTY_HTML_OPTS` does not name a class and stays in front.
#[test]
fn python_default_arg_constant_precedes_its_class() {
    use roundhouse::runtime_loader::python_units_subset;
    let units = python_units_subset(&["app/view_helpers.py"], |_path, classes| classes)
        .expect("python_units_subset");
    let body = &units[0].content;
    let const_at = body.find("EMPTY_HTML_OPTS = ").expect("EMPTY_HTML_OPTS");
    let class_at = body.find("class ViewHelpers").expect("ViewHelpers class");
    let def_at = body.find("def link_to(").expect("link_to");
    assert!(
        const_at < class_at && class_at < def_at,
        "EMPTY_HTML_OPTS must be bound before class ViewHelpers uses it as a default"
    );
}
