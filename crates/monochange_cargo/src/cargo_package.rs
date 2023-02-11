use std::path::PathBuf;

use cargo_metadata::PackageId;
use monochange_core::MonochangePackage;
use semver::Version;
use serde::Serialize;

#[derive(Serialize, Debug, Clone, Ord, Eq, PartialOrd, PartialEq)]
pub struct CargoPackage {
  #[serde(skip)]
  pub id: PackageId,
  pub name: String,
  pub version: Version,
  pub location: PathBuf,
  #[serde(skip)]
  pub path: PathBuf,
  pub private: bool,
  independent: bool,
}

impl MonochangePackage for CargoPackage {
  fn get_version(&self) -> Version {
    self.version.clone()
  }

  fn is_private(&self) -> bool {
    self.private
  }

  fn is_independent(&self) -> bool {
    self.independent
  }
}
