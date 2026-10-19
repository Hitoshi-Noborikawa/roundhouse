# frozen_string_literal: true
require_relative "../test_helper"
require "tmpdir"
require "spinel/active_support_cache"

class FrameworkSupportTest < Minitest::Test
  def test_isolated_execution_state_is_per_thread
    ActiveSupport::IsolatedExecutionState[:box] = { "n" => 1 }
    assert_equal({ "n" => 1 }, ActiveSupport::IsolatedExecutionState[:box])
    other = nil
    Thread.new { other = ActiveSupport::IsolatedExecutionState[:box] }.join
    assert_nil other
  end

  def test_notifications_fan_out_and_capture_sql
    seen = []
    subscriber = ActiveSupport::Notifications.subscribe(/demo/) do |name, _, _, _, payload|
      seen << [name, payload[:key]]
    end
    ActiveSupport::Notifications.instrument("demo.event", { key: "a" }) { "ran" }
    ActiveSupport::Notifications.unsubscribe(subscriber)
    assert_equal [["demo.event", "a"]], seen

    events = []
    ActiveSupport::Notifications.subscribe("once") { |event| events << event.payload[:key] }
    ran = ActiveSupport::Notifications.instrument("once", { key: "b" }) { "ran" }
    assert_equal "ran", ran
    assert_equal ["b"], events
    done = ActiveSupport::Notifications.subscribed(->(_, _, _, _, _) {}, "other", monotonic: true) { "done" }
    assert_equal "done", done
  end

  def test_null_store_does_not_keep_a_value
    store = ActiveSupport::Cache::NullStore.new
    assert_nil store.read("k")
    assert_equal true, store.write("k", "v")
    assert_nil store.read("k")
    assert_equal false, store.exist?("k")
    assert_equal "computed", store.fetch("k") { "computed" }
    assert_nil store.read("k")
    assert_nil store.read_str("k")
    assert_equal "fragment", store.fetch_str("k", 0) { "fragment" }
    assert_nil store.read_str("k")
  end

  def test_configuration_file_parses_yaml_and_erb
    path = File.join(Dir.tmpdir, "rh-config-#{Process.pid}.yml")
    File.write(path, "size: <%= 1 + 1 %>\n")
    loaded = ActiveSupport::ConfigurationFile.parse(path)
    assert_equal 2, loaded["size"]
  ensure
    File.delete(path) if path && File.exist?(path)
  end

end
