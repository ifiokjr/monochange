//! Repository model.

use typed_builder::TypedBuilder;
use welds::WeldsModel;

#[derive(Debug, Clone, WeldsModel, TypedBuilder)]
#[welds(table = "repositories")]
#[welds(BelongsTo(installation, super::installation::Installation, "installation_id"))]
#[welds(BeforeCreate(before_create_repository))]
#[welds(BeforeUpdate(before_update_repository))]
pub struct Repository {
	#[welds(primary_key)]
	#[welds(rename = "id")]
	#[builder(default)]
	pub id: i32,
	#[welds(rename = "installation_id")]
	pub installation_id: i32,
	#[welds(rename = "github_repo_id")]
	pub github_repo_id: i64,
	#[welds(rename = "github_full_name")]
	#[builder(setter(into))]
	pub github_full_name: String,
	#[welds(rename = "github_private")]
	#[builder(default)]
	pub github_private: bool,
	#[welds(rename = "monochange_config_hash")]
	#[builder(default, setter(into, strip_option(fallback = monochange_config_hash_opt)))]
	pub monochange_config_hash: Option<String>,
	#[welds(rename = "settings_json")]
	#[builder(default, setter(into, strip_option(fallback = settings_json_opt)))]
	pub settings_json: Option<String>,
	#[welds(rename = "plan_tier")]
	#[builder(default = "free".to_string(), setter(into))]
	pub plan_tier: String,
	#[welds(rename = "created_at")]
	#[builder(default = chrono::Utc::now())]
	pub created_at: chrono::DateTime<chrono::Utc>,
	#[welds(rename = "updated_at")]
	#[builder(default = chrono::Utc::now())]
	pub updated_at: chrono::DateTime<chrono::Utc>,
}

fn before_create_repository(model: &mut Repository) -> welds::errors::Result<()> {
	let now = chrono::Utc::now();
	model.created_at = now;
	model.updated_at = now;
	Ok(())
}

fn before_update_repository(model: &mut Repository) -> welds::errors::Result<()> {
	model.updated_at = chrono::Utc::now();
	Ok(())
}
