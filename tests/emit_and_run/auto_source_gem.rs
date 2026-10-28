use roundhouse::build_target::BuildTarget;

use super::auto_source_gem_contract as auto_source_gem;

fn mentions(tree: &std::path::Path, needle: &str) -> bool {
    walkdir(tree).iter().any(|p| std::fs::read_to_string(p).is_ok_and(|text| text.contains(needle)))
}

fn walkdir(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walkdir(&path));
        } else if path.extension().is_some_and(|e| e == "rb") {
            out.push(path);
        }
    }
    out
}

#[test]
fn the_spinel_tree_compiles_the_installed_gem_and_its_dependency() {
    let (emitted, errors) = auto_source_gem::overlay().emit(BuildTarget::Spinel);
    assert!(errors.is_empty(), "{errors:#?}");
    assert!(mentions(&emitted, "module Greetings"), "greetings is not in the Spinel tree");
    assert!(mentions(&emitted, "module Exclaim"), "exclaim is not in the Spinel tree");
    // Spelled whole: runtime/multipart.rb has `def self.boundary`.
    assert!(!mentions(&emitted, "class Bindings\n"), "an unreached gem class is in the Spinel tree");
    assert!(!mentions(&emitted, "def self.bound(values)"), "an unreached gem method is in the Spinel tree");
    assert!(mentions(&emitted, "@width = value.to_i * 2"), "a writer reached by a built name is not in the Spinel tree");
}

#[test]
fn the_ruby_tree_loads_the_real_gem_instead() {
    let (emitted, errors) = auto_source_gem::overlay().emit(BuildTarget::Ruby);
    assert!(errors.is_empty(), "{errors:#?}");
    assert!(!mentions(&emitted, "module Greetings"), "greetings is compiled into the Ruby tree");
    assert!(!mentions(&emitted, "module Exclaim"), "exclaim is compiled into the Ruby tree");
    let gemfile = std::fs::read_to_string(emitted.join("Gemfile")).expect("Gemfile");
    assert!(gemfile.contains("gem \"greetings\""), "{gemfile}");
}
