module AssignmentHooks
  attr_reader :status_input, :settings_input

  def status=(value)
    @status_input = value
    super(value)
  end

  def settings=(value)
    @settings_input = value
    super(value)
  end
end
