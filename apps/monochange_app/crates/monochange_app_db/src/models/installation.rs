//! Installation model — GitHub App installations.

use typed_builder::TypedBuilder;
use welds::WeldsModel;

#[derive(Debug, Clone, WeldsModel, TypedBuilder)]
#[welds(table = "installations")]
#[welds(BelongsTo(user, super::user::User, "user_id"))]
#[welds(HasMany(repos, super::repository::Repository, "installation_id"))]
#[welds(BeforeCreate(before_create_installation))]
#[welds(BeforeUpdate(before_update_installation))]
pub struct Installation {
	#[welds(primary_key)]
	#[welds(rename = "id")]
	#[builder(default)]
	pub id: i32,
	#[welds(rename = "user_id")]
	pub user_id: i32,
	#[welds(rename = "github_installation_id")]
	pub github_installation_id: i64,
	#[welds(rename = "github_account_login")]
	#[builder(setter(into))]
	pub github_account_login: String,
	#[welds(rename = "github_account_type")]
	#[builder(setter(into))]
	pub github_account_type: String,
	#[welds(rename = "target_type")]
	#[builder(default = "selected".to_string(), setter(into))]
	pub target_type: String,
	#[welds(rename = "created_at")]
	#[builder(default = chrono::Utc::now())]
	pub created_at: chrono::DateTime<chrono::Utc>,
	#[welds(rename = "updated_at")]
	#[builder(default = chrono::Utc::now())]
	pub updated_at: chrono::DateTime<chrono::Utc>,
}

fn before_create_installation(model: &mut Installation) -> welds::errors::Result<()> {
	let now = chrono::Utc::now();
	model.created_at = now;
	model.updated_at = now;
	Ok(())
}

fn before_update_installation(model: &mut Installation) -> welds::errors::Result<()> {
	model.updated_at = chrono::Utc::now();
	Ok(())
}
