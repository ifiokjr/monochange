Gem::Specification.new do |spec|
  spec.name = "api"
  spec.version = Api::VERSION
  spec.summary = "API layer"

  spec.add_dependency "core", "~> 2.0"
  spec.add_dependency "sinatra", "~> 3.0"
  spec.add_development_dependency "rack-test", "~> 2.0"
end
