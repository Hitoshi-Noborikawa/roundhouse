//! Rails 8.1's exception constants are values, raise arguments, rescue
//! classes and type guards. The emitted runtime must define the same
//! names and parents, including the service/preview/transformer errors.

use super::emit_and_run;

const EXCEPTIONS: &[(&str, &str)] = &[
    ("ActiveStorage::Error", "StandardError"),
    ("ActiveStorage::InvariableError", "ActiveStorage::Error"),
    ("ActiveStorage::UnpreviewableError", "ActiveStorage::Error"),
    ("ActiveStorage::UnrepresentableError", "ActiveStorage::Error"),
    ("ActiveStorage::IntegrityError", "ActiveStorage::Error"),
    ("ActiveStorage::FileNotFoundError", "ActiveStorage::Error"),
    ("ActiveStorage::PreviewError", "ActiveStorage::Error"),
    ("ActiveStorage::InvalidKeyError", "ActiveStorage::Error"),
    ("ActiveStorage::Preview::UnprocessedError", "StandardError"),
    (
        "ActiveStorage::Service::GCSService::MetadataServerError",
        "ActiveStorage::Error",
    ),
    (
        "ActiveStorage::Service::GCSService::MetadataServerNotFoundError",
        "ActiveStorage::Error",
    ),
    (
        "ActiveStorage::Transformers::ImageProcessingTransformer::UnsupportedImageProcessingMethod",
        "StandardError",
    ),
    (
        "ActiveStorage::Transformers::ImageProcessingTransformer::UnsupportedImageProcessingArgument",
        "StandardError",
    ),
];

fn app(methods: &str) -> emit_and_run::Overlay {
    emit_and_run::empty_app()
        .write("db/schema.rb", "ActiveRecord::Schema[8.1].define do\nend\n")
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
        .write(
            "app/models/application_record.rb",
            "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n",
        )
        .write(
            "app/models/storage_errors.rb",
            &format!("class StorageErrors < ApplicationRecord\n{methods}\nend\n"),
        )
}

#[test]
fn active_storage_exception_constant_values_keep_their_names_and_parents() {
    let names = EXCEPTIONS.iter().map(|(name, _)| *name).collect::<Vec<_>>().join(", ");
    let expected = EXCEPTIONS
        .iter()
        .map(|(name, parent)| format!("[\"{name}\", \"{parent}\"]"))
        .collect::<Vec<_>>()
        .join(", ");
    app(&format!("  def self.classes\n    [{names}]\n  end"))
        .run_ruby(&format!(
            "actual = StorageErrors.classes.map {{ |klass| [klass.name, klass.superclass.name] }}\n\
             raise actual.inspect unless actual == [{expected}]"
        ))
        .assert_passes();
}

#[test]
fn active_storage_exception_owners_can_be_reopened_with_rails_superclasses() {
    emit_and_run::empty_app()
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
        .write("db/schema.rb", "ActiveRecord::Schema[8.1].define do\nend\n")
        .write("app/models/storage_owners.rb", r#"module ActiveStorage
  module Transformers
    def self.marker
      "transformers"
    end
    class Transformer
    end
    class ImageProcessingTransformer < Transformer
      def self.marker
        "custom transformer"
      end
    end
  end
  class Service
    class GCSService < Service
      def self.marker
        "custom service"
      end
    end
  end
end
"#)
        .run_ruby(r#"
raise unless ActiveStorage::Transformers.marker == "transformers"
raise unless ActiveStorage::Transformers::ImageProcessingTransformer.marker == "custom transformer"
raise unless ActiveStorage::Service::GCSService.marker == "custom service"
raise unless ActiveStorage::Transformers::ImageProcessingTransformer.superclass == ActiveStorage::Transformers::Transformer
raise unless ActiveStorage::Transformers::Transformer.superclass == Object
raise unless ActiveStorage::Service::GCSService.superclass == ActiveStorage::Service
"#)
        .assert_passes();
}

#[test]
fn active_storage_exception_owner_values_have_the_rails_hierarchy() {
    app("  def self.owners\n    [ActiveStorage::Transformers, ActiveStorage::Transformers::Transformer,\n     ActiveStorage::Transformers::ImageProcessingTransformer, ActiveStorage::Service::GCSService]\n  end")
        .run_ruby(r#"
owners = StorageErrors.owners
raise owners.inspect unless owners.map(&:name) == [
  "ActiveStorage::Transformers", "ActiveStorage::Transformers::Transformer",
  "ActiveStorage::Transformers::ImageProcessingTransformer", "ActiveStorage::Service::GCSService"
]
raise unless owners[0].instance_of?(Module)
raise unless owners[1].superclass == Object
raise unless owners[2].superclass == owners[1]
raise unless owners[3].superclass == ActiveStorage::Service
"#)
        .assert_passes();
}

#[test]
fn active_storage_exception_owners_support_qualified_reopens_and_subclasses() {
    emit_and_run::empty_app()
        .write("config/routes.rb", "Rails.application.routes.draw do\nend\n")
        .write("db/schema.rb", "ActiveRecord::Schema[8.1].define do\nend\n")
        .write("app/models/storage_owners.rb", r#"class ActiveStorage::Transformers::ImageProcessingTransformer < ::ActiveStorage::Transformers::Transformer
  def self.marker
    "qualified transformer"
  end
end
class ActiveStorage::Service::GCSService < ::ActiveStorage::Service
  def self.marker
    "qualified service"
  end
end
class CustomTransformer < ActiveStorage::Transformers::ImageProcessingTransformer
end
class CustomService < ActiveStorage::Service::GCSService
end
"#)
        .run_ruby(r#"
raise unless CustomTransformer.marker == "qualified transformer"
raise unless CustomService.marker == "qualified service"
raise unless CustomTransformer.superclass.superclass == ActiveStorage::Transformers::Transformer
raise unless CustomService.superclass.superclass == ActiveStorage::Service
TransformerBase = ActiveStorage::Transformers::ImageProcessingTransformer
class AliasedTransformer < TransformerBase
end
ServiceBase = ActiveStorage::Service::GCSService
class AliasedService < ServiceBase
end
raise unless AliasedTransformer.marker == "qualified transformer"
raise unless AliasedService.marker == "qualified service"
raise unless ActiveStorage::Transformers::ImageProcessingTransformer::UnsupportedImageProcessingMethod.superclass == StandardError
raise unless ActiveStorage::Service::GCSService::MetadataServerError.superclass == ActiveStorage::Error
"#)
        .assert_passes();
}

#[test]
fn active_storage_exceptions_in_rescue_have_the_exception_surface() {
    let mut methods = String::new();
    let mut script = String::new();
    for (i, (name, parent)) in EXCEPTIONS.iter().enumerate() {
        methods.push_str(&format!(
            "  def self.rescued_{i}\n    raise ::{name}, \"storage failure\"\n\
             rescue {name} => error\n    error.message.upcase\n  end\n\
             def self.parent_rescue_{i}\n    raise {name}, \"parent rescue\"\n\
             rescue ::{parent} => error\n    error.message.upcase\n  end\n"
        ));
        script.push_str(&format!(
            "raise \"rescue {name}\" unless StorageErrors.rescued_{i} == \"STORAGE FAILURE\"\n\
             raise \"parent {name}\" unless StorageErrors.parent_rescue_{i} == \"PARENT RESCUE\"\n"
        ));
    }
    app(&methods).run_ruby(&script).assert_passes();
}

#[test]
fn active_storage_exceptions_can_be_raised_as_classes_or_instances() {
    let mut methods = String::new();
    let mut script = String::new();
    for (i, (name, _)) in EXCEPTIONS.iter().enumerate() {
        methods.push_str(&format!(
            "  def self.raise_class_{i}\n    raise {name}, \"storage failure\"\n  end\n\
             def self.raise_instance_{i}\n    raise ::{name}.new(\"storage failure\")\n  end\n"
        ));
        for form in ["class", "instance"] {
            script.push_str(&format!(
                "begin\n  StorageErrors.raise_{form}_{i}\nrescue {name} => error\n\
                 raise error.inspect unless error.class == {name} && error.message == \"storage failure\"\n\
                 else\n  raise \"did not raise {name}\"\nend\n"
            ));
        }
    }
    app(&methods).run_ruby(&script).assert_passes();
}

#[test]
fn active_storage_exception_is_a_guards_narrow_to_the_exception_surface() {
    let mut methods = String::new();
    let mut script = String::new();
    for (i, (name, _)) in EXCEPTIONS.iter().enumerate() {
        methods.push_str(&format!(
            "  def self.match_{i}(error)\n    if error.is_a?(::{name})\n\
                 error.message.upcase\n    else\n      \"different\"\n    end\n  end\n"
        ));
        script.push_str(&format!(
            "raise \"match {name}\" unless StorageErrors.match_{i}({name}.new(\"storage failure\")) == \"STORAGE FAILURE\"\n\
             raise \"nonmatch {name}\" unless StorageErrors.match_{i}(StandardError.new) == \"different\"\n"
        ));
    }
    app(&methods).run_ruby(&script).assert_passes();
}

#[test]
fn an_unknown_active_storage_exception_stays_refused() {
    let errors = app("  def self.unknown\n    ActiveStorage::UnknownStorageError\n  end").check_errors();
    assert!(
        errors.iter().any(|e| e.contains("ActiveStorage::UnknownStorageError")),
        "{errors:#?}"
    );
}

#[test]
fn active_storage_exception_methods_are_still_checked() {
    let root = std::env::temp_dir().join(format!("roundhouse-storage-methods-{}", std::process::id()));
    std::fs::create_dir_all(root.join("app/models")).unwrap();
    std::fs::write(
        root.join("app/models/application_record.rb"),
        "class ApplicationRecord < ActiveRecord::Base\n  self.abstract_class = true\nend\n",
    )
    .unwrap();
    std::fs::write(
        root.join("app/models/storage_errors.rb"),
        "class StorageErrors < ApplicationRecord\n  def checked\n    raise \"failure\"\n\
         rescue ActiveStorage::FileNotFoundError => error\n    error.no_such_exception_method\n  end\nend\n",
    )
    .unwrap();
    let mut app = roundhouse::ingest::ingest_app(&root).unwrap();
    roundhouse::analyze::Analyzer::new(&app).analyze(&mut app);
    let diagnostics = roundhouse::analyze::diagnose(&app);
    std::fs::remove_dir_all(root).unwrap();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code() == "send_dispatch_failed" && d.message.contains("no_such_exception_method")),
        "{diagnostics:#?}"
    );
    assert!(
        !diagnostics.iter().any(|d| d.message.contains("constant not supported")),
        "{diagnostics:#?}"
    );
}
