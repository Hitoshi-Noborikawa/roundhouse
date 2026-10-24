//! Cable::Hub (runtime/spinel/scaffold/ruby_overlay/runtime/cable_hub.rb):
//! a broadcast in one forked worker reaches the sockets of every other
//! worker, once, with String and Hash messages intact and one publisher's
//! messages in order.

use std::process::Command;

const HUB_ROUND_TRIP: &str = r#"
require "tmpdir"
require File.expand_path("runtime/spinel/scaffold/ruby_overlay/runtime/cable_hub", Dir.pwd)

# A worker's local Registry, recording what reaches its sockets.
class Local
  attr_reader :got
  def initialize = @got = Queue.new
  def broadcast(stream, message) = @got << [stream, message]
end

def take(local, n)
  Array.new(n) { Timeout.timeout(5) { local.got.pop } }
end
require "timeout"

path = File.join(Dir.tmpdir, "cable-hub-test-#{Process.pid}.sock")
hub = fork { Cable::Hub.serve(path) }
begin
  a, b, c = Local.new, Local.new, Local.new
  # Connect before the hub listens: connect waits for it.
  workers = [a, b, c].map { |local| Cable::Hub.connect(path, local) }
  sleep 0.2 # all three accepted before the first publish

  html = %(<turbo-stream action="append" target="messages"><template><p>hi\nthere</p></template></turbo-stream>)
  workers[0].broadcast("room:1", html)
  workers[0].broadcast("user_7_unreads", { roomId: 1, at: 2.5 })
  20.times { |i| workers[0].broadcast("room:1", "m#{i}") }

  # The sender delivers to its own sockets directly, once.
  sent = take(a, 22)
  raise "sender order" unless sent.map(&:last) == [html, { roomId: 1, at: 2.5 }, *Array.new(20) { |i| "m#{i}" }]

  [b, c].each do |local|
    got = take(local, 22)
    raise "fragment: #{got[0].inspect}" unless got[0] == ["room:1", html]
    raise "payload: #{got[1].inspect}" unless got[1] == ["user_7_unreads", { "roomId" => 1, "at" => 2.5 }]
    raise "order: #{got[2..].map(&:last).inspect}" unless got[2..].map(&:last) == Array.new(20) { |i| "m#{i}" }
  end
  sleep 0.2
  raise "duplicate delivery" unless [a, b, c].all? { |local| local.got.empty? }

  # A second publisher reaches the first.
  workers[2].broadcast("room:2", "from c")
  raise "reverse" unless take(a, 1) == [["room:2", "from c"]] && take(b, 1) == [["room:2", "from c"]]
  puts "ALL OK"
ensure
  Process.kill(:TERM, hub)
  Process.wait(hub)
  File.unlink(path) if File.exist?(path)
end
"#;

#[test]
fn hub_relays_every_broadcast_to_the_other_workers_once_and_in_order() {
    let output = Command::new("ruby")
        .args(["-e", HUB_ROUND_TRIP])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("ruby is on PATH");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "hub round trip failed:\n{stdout}\n{stderr}");
    assert!(stdout.contains("ALL OK"), "hub cases did not execute:\n{stdout}\n{stderr}");
}
