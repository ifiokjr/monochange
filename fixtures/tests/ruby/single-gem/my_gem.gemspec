Gem::Specification.new do |spec|
  spec.name = "my_gem"
  spec.version = MyGem::VERSION
  spec.summary = "A sample gem"
  spec.authors = ["Test Author"]

  spec.add_dependency "rails", "~> 7.0"
  spec.add_dependency "redis", ">= 4.0"
  spec.add_development_dependency "rspec", "~> 3.0"
  spec.add_development_dependency "rubocop", ">= 1.0"
end
