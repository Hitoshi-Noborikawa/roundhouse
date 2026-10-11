use std::collections::HashMap;
use std::path::PathBuf;

use roundhouse::gems::statically_resolved_external_includes;
use roundhouse::ingest::ingest_app_from_tree;
use roundhouse::project::{self, BuildTarget};

fn app(source: &str) -> roundhouse::App {
    let files = [
        (
            "Gemfile.lock",
            "GEM\n  specs:\n    redcarpet (3.6.0)\n    rouge (4.5.1)\n\nDEPENDENCIES\n  redcarpet\n  rouge\n",
        ),
        ("lib/markdown_renderer.rb", source),
    ];
    let tree: HashMap<PathBuf, Vec<u8>> = files
        .into_iter()
        .map(|(path, source)| (PathBuf::from(path), source.as_bytes().to_vec()))
        .collect();
    ingest_app_from_tree(tree).expect("ingest external gem include")
}

#[test]
fn locked_external_include_keeps_its_literal_feature_require() {
    let app = app("require \"rouge/plugins/redcarpet\"\n\
         class MarkdownRenderer < Redcarpet::Render::HTML\n\
           include Rouge::Plugins::Redcarpet\n\
         end\n");

    let resolved = statically_resolved_external_includes(&app);
    assert_eq!(
        resolved.get("MarkdownRenderer includes unresolved Rouge::Plugins::Redcarpet"),
        Some(&vec!["rouge/plugins/redcarpet".to_string()]),
        "only the locked gem plus exact literal require resolves the include"
    );

    let files = project::target_files(&app, &PathBuf::from("."), BuildTarget::Ruby)
        .expect("emit Ruby project");
    let renderer = files
        .iter()
        .find(|(_, content)| content.contains("class MarkdownRenderer"))
        .expect("emitted MarkdownRenderer");
    assert!(
        renderer
            .1
            .starts_with("require \"rouge/plugins/redcarpet\"\n"),
        "external plugin load must precede the class definition:\n{}",
        renderer.1
    );
    assert!(renderer.1.contains("include Rouge::Plugins::Redcarpet"));

    let gemfile = files
        .iter()
        .find(|(path, _)| path == "Gemfile")
        .expect("generated Gemfile");
    assert!(gemfile.1.contains("gem \"redcarpet\", \"= 3.6.0\""));
    assert!(gemfile.1.contains("gem \"rouge\", \"= 4.5.1\""));
    let boot = files
        .iter()
        .find(|(path, _)| path == "boot.rb")
        .expect("generated boot.rb");
    assert!(boot.1.contains("require \"redcarpet\""));
    assert!(boot.1.contains("require \"rouge\""));
    assert!(!files.iter().any(|(path, _)| path == "Gemfile.lock"));
}

#[test]
fn unrequired_or_nonliteral_external_includes_remain_unresolved() {
    let mut missing_lock = app("require \"rouge/plugins/redcarpet\"\n\
         class MarkdownRenderer < Redcarpet::Render::HTML\n\
           include Rouge::Plugins::Redcarpet\n\
         end\n");
    missing_lock.gem_lock = None;
    assert!(statically_resolved_external_includes(&missing_lock).is_empty());

    let unrequired = app("class MarkdownRenderer < Redcarpet::Render::HTML\n\
           include Rouge::Plugins::Redcarpet\n\
         end\n");
    assert!(statically_resolved_external_includes(&unrequired).is_empty());

    let dynamic = app("feature = \"rouge/plugins/redcarpet\"\n\
         require feature\n\
         class MarkdownRenderer < Redcarpet::Render::HTML\n\
           include Rouge::Plugins::Redcarpet\n\
         end\n");
    assert!(statically_resolved_external_includes(&dynamic).is_empty());

    let conditional = app(
        "require \"rouge/plugins/redcarpet\" if ENV[\"ENABLE_MARKDOWN\"]\n\
         class MarkdownRenderer < Redcarpet::Render::HTML\n\
           include Rouge::Plugins::Redcarpet\n\
         end\n",
    );
    assert!(statically_resolved_external_includes(&conditional).is_empty());
}
