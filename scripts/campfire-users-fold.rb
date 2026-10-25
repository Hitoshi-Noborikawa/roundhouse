# scripts/campfire-users-fold.rb OUT LANE... — folds bench-campfire's users
# phase into OUT/users.json: each lane's `loadgen users` result, with the
# peak PSS/RSS its server reached in each step's window and the cores its
# process tree used there (the samples are unix-ms stamped; so are the
# loadgen's PHASE lines).
require "json"

out, *lanes = ARGV
fold = {}
lanes.each do |lane|
  path = File.join(out, "users-#{lane}.json")
  next unless File.exist?(path) && !File.zero?(path)
  result = JSON.parse(File.read(path))
  log = File.join(out, "logs", "users-loadgen-#{lane}.log")
  phases = File.exist?(log) ? File.readlines(log).grep(/\APHASE /).map { |l| l.split.then { |p| [p[1..-2].join(" "), p[-1].to_i] } } : []
  mem = File.exist?(File.join(out, "users-memory-#{lane}.tsv")) ?
    File.readlines(File.join(out, "users-memory-#{lane}.tsv")).map { |l| l.split("\t").map(&:to_i) } : []
  starts = phases.each_with_index.select { |(name, _), _| name.start_with?("step ") && !name.include?("warmup") }
  result["steps"].each_with_index do |step, i|
    start = starts[i] or next
    idx = start[1]
    from = phases[idx][1]
    to = phases[idx + 1]&.at(1) || Float::INFINITY
    window = mem.select { |t, _, _| t >= from && t < to }
    step["memory"] = { "peak_pss_mb" => (window.map { |r| r[1] }.max.to_i / 1024.0).round(1),
                       "peak_rss_mb" => (window.map { |r| r[2] }.max.to_i / 1024.0).round(1) } unless window.empty?
    # Cores the server's process tree used over the window (USER_HZ ticks).
    timed = window.select { |r| r.length > 3 }
    if timed.length >= 2 && timed.last[0] > timed.first[0]
      step["server_cpu_cores"] = ((timed.last[3] - timed.first[3]) / 100.0 / ((timed.last[0] - timed.first[0]) / 1000.0)).round(2)
    end
  end
  fold[lane] = result
end
File.write(File.join(out, "users.json"), JSON.pretty_generate(fold) + "\n") unless fold.empty?
