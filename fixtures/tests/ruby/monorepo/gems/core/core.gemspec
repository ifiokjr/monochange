Gem::Specification.new do |s|
  s.name = "core"
  s.version = Core::VERSION
  s.summary = "Core library"

  s.add_dependency "activesupport", "~> 7.0"
  s.add_runtime_dependency "concurrent-ruby", "~> 1.2"
end
