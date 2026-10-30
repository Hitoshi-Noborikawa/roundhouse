# frozen_string_literal: true
module Rack
  class MiniProfiler
    # Not the gem's attribute Hash behind `[]`: spinel types a Hash by its values, and these mix Strings, Integers, Floats and timers.
    module TimerStruct
      # `::JSON.generate`'s escaping, which leaves `<`, `>` and `&` alone, unlike the ActiveSupport encoder in JsonBuilder.
      def self.json_string(s)
        out = +"\""
        i = 0
        n = s.bytesize
        while i < n
          b = s.getbyte(i)
          if b == 0x22
            out << "\\\""
          elsif b == 0x5C
            out << "\\\\"
          elsif b == 0x0A
            out << "\\n"
          elsif b == 0x0D
            out << "\\r"
          elsif b == 0x09
            out << "\\t"
          elsif b == 0x08
            out << "\\b"
          elsif b == 0x0C
            out << "\\f"
          elsif b < 0x20
            out << "\\u00" + HEX[b / 16] + HEX[b % 16]
          else
            out << s.byteslice(i, 1)
          end
          i += 1
        end
        out << "\""
        out
      end

      HEX = "0123456789abcdef"

      class Request
        attr_reader :id, :name, :start_milliseconds, :duration_milliseconds, :is_trivial

        def initialize(name, page_started)
          @id = MiniProfiler.generate_id
          @name = name
          @duration_milliseconds = 0.0
          @start_milliseconds = (Process.clock_gettime(Process::CLOCK_MONOTONIC) * 1000).to_i - page_started
          @is_trivial = false
        end

        def record_time(milliseconds)
          @duration_milliseconds = milliseconds
          @is_trivial = true if milliseconds < 2
          nil
        end

        def to_json
          "{\"id\":" + TimerStruct.json_string(@id) +
            ",\"name\":" + TimerStruct.json_string(@name) +
            ",\"duration_milliseconds\":" + @duration_milliseconds.to_s +
            ",\"duration_without_children_milliseconds\":" + @duration_milliseconds.to_s +
            ",\"start_milliseconds\":" + @start_milliseconds.to_s +
            ",\"parent_timing_id\":null,\"children\":[],\"has_children\":false,\"key_values\":null" +
            ",\"has_sql_timings\":false,\"has_duplicate_sql_timings\":false" +
            ",\"trivial_duration_threshold_milliseconds\":2,\"sql_timings\":[]" +
            ",\"sql_timings_duration_milliseconds\":0,\"is_trivial\":" + @is_trivial.to_s +
            ",\"is_root\":true,\"depth\":0,\"executed_readers\":0,\"executed_scalars\":0" +
            ",\"executed_non_queries\":0,\"custom_timing_stats\":{},\"custom_timings\":{}}"
        end
      end

      class Page
        # The stored JSON is rewritten in place on the first view (`Page.viewed`), so these two keys are written side by side.
        UNVIEWED = "\"has_user_viewed\":false,\"client_timings\":null"

        attr_reader :id, :root, :started, :user

        def initialize(id, name, request_method, request_path, machine_name, root_name, user)
          @id = id
          @name = name
          @started_at = (Time.now.to_f * 1000).to_i
          @started = (Process.clock_gettime(Process::CLOCK_MONOTONIC) * 1000).to_i
          @machine_name = machine_name
          @request_method = request_method
          @request_path = request_path
          @user = user
          @root = Request.new(root_name, @started)
        end

        def duration_ms
          @root.duration_milliseconds
        end

        def to_json
          "{\"id\":" + TimerStruct.json_string(@id) +
            ",\"name\":" + TimerStruct.json_string(@name) +
            ",\"started\":" + @started.to_s +
            ",\"started_at\":" + @started_at.to_s +
            ",\"machine_name\":" + TimerStruct.json_string(@machine_name) +
            ",\"level\":0,\"user\":" + TimerStruct.json_string(@user) +
            "," + UNVIEWED +
            ",\"duration_milliseconds\":" + @root.duration_milliseconds.to_s +
            ",\"has_trivial_timings\":true,\"has_all_trivial_timings\":false" +
            ",\"trivial_duration_threshold_milliseconds\":2,\"head\":null,\"sql_count\":0" +
            ",\"cached_sql_count\":0,\"duration_milliseconds_in_sql\":0,\"has_sql_timings\":true" +
            ",\"has_duplicate_sql_timings\":false,\"executed_readers\":0,\"executed_scalars\":0" +
            ",\"executed_non_queries\":0,\"custom_timing_names\":[],\"custom_timing_stats\":{}" +
            ",\"custom_fields\":{},\"has_flamegraph\":false" +
            ",\"request_method\":" + TimerStruct.json_string(@request_method) +
            ",\"request_path\":" + TimerStruct.json_string(@request_path) +
            ",\"root\":" + @root.to_json +
            ",\"started_formatted\":\"/Date(" + @started_at.to_s + ")/\"}"
        end

        def self.viewed?(json)
          !json.include?(UNVIEWED)
        end

        def self.viewed(json, client_timings)
          # Not `sub` with a String: it reads the replacement's backslashes as references, and the client timings are request data.
          at = json.index(UNVIEWED)
          return json if at.nil?
          json[0, at] + "\"has_user_viewed\":true,\"client_timings\":" + client_timings + json[at + UNVIEWED.length, json.length - at - UNVIEWED.length]
        end

        # Page#duration_ms and #name read back from the stored JSON, for the share page.
        def self.read_name(json)
          read_string(json, "\"name\":")
        end

        def self.read_duration(json)
          key = "\"duration_milliseconds\":"
          at = json.index(key)
          return 0.0 if at.nil?
          from = at + key.length
          stop = json.index(",", from)
          return 0.0 if stop.nil?
          json[from, stop - from].to_f
        end

        def self.read_string(json, key)
          at = json.index(key)
          return "" if at.nil?
          i = at + key.length + 1
          out = +""
          while i < json.length && json[i] != "\""
            if json[i] == "\\"
              i += 1
              c = json[i]
              out << (c == "n" ? "\n" : c == "t" ? "\t" : c == "r" ? "\r" : c)
            else
              out << json[i]
            end
            i += 1
          end
          out
        end
      end

      class Client
        def self.from_form(fields)
          base = fields.fetch("clientPerformance[timing][navigationStart]", "").to_i
          timing = [""]
          timing.pop
          navigation = false
          fields.each do |k, _v|
            if k.start_with?("clientPerformance[timing][") && k.end_with?("]")
              timing << k["clientPerformance[timing][".length, k.length - "clientPerformance[timing][".length - 1]
            end
            navigation = true if k.start_with?("clientPerformance[navigation][")
          end
          return "null" if timing.empty?
          # The gem reads `form['clientPerformance']['navigation']['redirect_count']` unguarded, so a form without it is a 500 there too.
          raise NoMethodError, "undefined method '[]' for nil" unless navigation
          out = [""]
          out.pop
          timing.each do |k|
            next unless k.end_with?("Start")
            start = fields["clientPerformance[timing][" + k + "]"].to_i - base
            finish = fields.fetch("clientPerformance[timing][" + k.sub(/Start\z/, "End") + "]", "").to_i - base
            duration = finish > start ? finish - start : 0
            out << timing_json(Client.words(k.sub(/Start\z/, "")), start, duration) if start >= 0
          end
          timing.each do |k|
            next if k.end_with?("Start") || k.end_with?("End")
            out << timing_json(k, fields["clientPerformance[timing][" + k + "]"].to_i - base, -1)
          end
          redirect = fields.key?("clientPerformance[navigation][redirect_count]") ? TimerStruct.json_string(fields["clientPerformance[navigation][redirect_count]"]) : "null"
          "{\"redirect_count\":" + redirect + ",\"timings\":[" + out.join(",") + "]}"
        end

        # `split(/(?=[A-Z])/)` then capitalize: "domContentLoadedEvent" is "Dom Content Loaded Event".
        def self.words(name)
          out = +""
          i = 0
          while i < name.length
            c = name[i]
            if i == 0
              out << c.upcase
            elsif c >= "A" && c <= "Z"
              out << " " << c
            else
              out << c
            end
            i += 1
          end
          out
        end

        def self.timing_json(name, start, duration)
          "{\"Name\":" + TimerStruct.json_string(name) + ",\"Start\":" + start.to_s + ",\"Duration\":" + duration.to_s + "}"
        end
      end
    end
  end
end
