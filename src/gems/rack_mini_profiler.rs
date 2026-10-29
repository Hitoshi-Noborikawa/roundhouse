//! rack-mini-profiler on the Spinel tree: `runtime/gems/rack_mini_profiler/`.

pub(super) const PACKAGE: super::GemPackage = super::GemPackage {
    gem: "rack-mini-profiler",
    dir: "rack_mini_profiler",
    entry: "rack_mini_profiler",
    config_receiver: Some("Rack::MiniProfiler.config"),
};
