class LocalDurationClock
  def self.added
    at = Time.utc(2026, 1, 2, 3, 4, 5, 123456)
    delay = 2.seconds
    at + delay
  end

  def self.subtracted
    at = Time.utc(2026, 1, 2, 3, 4, 5, 123456)
    delay = 2.minutes
    copy = delay
    at - copy
  end

  def self.retained_duration
    at = Time.utc(2026, 1, 2)
    delay = 2.hours
    later = at + delay
    raise "duration changed" unless delay.to_i == 7200
    later
  end
end
