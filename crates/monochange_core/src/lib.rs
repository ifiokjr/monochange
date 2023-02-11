use semver::Version;

/// This the plugin API for monochange. Every plugin must implement this trait.
pub trait MonochangePlugin {
  type Package: MonochangePackage;
  fn get_packages(&self) -> Vec<Self::Package>;
}

/// Implement this trait for your package.
pub trait MonochangePackage {
  /// Get the semver version of this package.
  fn get_version(&self) -> Version;

  /// Check whether this package is private.
  fn is_private(&self) -> bool;

  /// Check whether this package is independent. The version is not locked to
  /// the other packages.
  fn is_independent(&self) -> bool;
}

pub enum VersionRule {
  /// The version is locked to the other packages.
  Locked,
  /// The version is not locked to the other packages.
  Independent,
  /// The version is part of a version group. Each group has an id.
  Grouped(String),
}
