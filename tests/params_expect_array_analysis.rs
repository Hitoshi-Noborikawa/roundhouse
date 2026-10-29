use std::collections::HashMap;
use std::path::PathBuf;

fn source_errors(body: &str) -> Vec<String> {
    let controller = format!(
        "class AnswersController < ApplicationController\n  def create\n{body}\n  end\nend\n"
    );
    let tree: HashMap<PathBuf, Vec<u8>> = [
        (
            "db/schema.rb",
            "ActiveRecord::Schema[8.1].define(version: 1) do\nend\n",
        ),
        (
            "app/controllers/application_controller.rb",
            "class ApplicationController < ActionController::Base\nend\n",
        ),
        ("app/controllers/answers_controller.rb", controller.as_str()),
        (
            "config/routes.rb",
            "Rails.application.routes.draw do\n  resources :answers, only: :create\nend\n",
        ),
    ]
    .into_iter()
    .map(|(path, source)| (PathBuf::from(path), source.as_bytes().to_vec()))
    .collect();
    let mut app = roundhouse::ingest::ingest_app_from_tree(tree).unwrap();
    roundhouse::analyze::Analyzer::new(&app).analyze(&mut app);
    roundhouse::analyze::diagnose(&app)
        .into_iter()
        .filter(|d| d.severity == roundhouse::diagnostic::Severity::Error)
        .map(|d| d.to_string())
        .collect()
}

#[test]
fn array_expect_is_an_array_before_controller_lowering() {
    for receiver in ["params", "self.params", "@params"] {
        let initialization = if receiver == "@params" {
            "    @params = params\n"
        } else {
            ""
        };
        let errors = source_errors(&format!(
            "{initialization}    answers = {receiver}.expect(answers: [[:header, :other, {{ labels: [] }}]])\n    render json: answers.map(&:to_h)",
        ));
        assert!(errors.is_empty(), "{receiver}: {errors:?}");
    }
}

#[test]
fn unsupported_nested_filters_keep_the_source_error() {
    let errors = source_errors(
        "    answers = params.expect(answers: [[:header, { records: [[:name]] }]])\n    render json: answers.map(&:to_h)",
    );
    assert!(
        errors.iter().any(|error| error.contains("map")),
        "{errors:?}"
    );
}

#[test]
fn an_ordinary_hash_does_not_gain_the_params_array_contract() {
    let errors = source_errors(
        "    answers = { value: \"scalar\" }.expect(answers: [[:header]])\n    render json: answers.map(&:to_h)",
    );
    assert!(
        errors.iter().any(|error| error.contains("map")),
        "{errors:?}"
    );
}
