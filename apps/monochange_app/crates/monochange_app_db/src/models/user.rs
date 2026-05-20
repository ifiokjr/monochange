//! User model.

use typed_builder::TypedBuilder;
use welds::WeldsModel;

#[derive(Debug, Clone, WeldsModel, TypedBuilder)]
#[welds(table = "users")]
#[welds(HasMany(installations, super::installation::Installation, "user_id"))]
#[welds(HasMany(org_memberships, super::organization::OrganizationMember, "user_id"))]
#[welds(BeforeCreate(before_create_user))]
#[welds(BeforeUpdate(before_update_user))]
pub struct User {
	#[welds(primary_key)]
	#[welds(rename = "id")]
	#[builder(default)]
	pub id: i32,
	#[welds(rename = "github_id")]
	pub github_id: i64,
	#[welds(rename = "github_login")]
	#[builder(setter(into))]
	pub github_login: String,
	#[welds(rename = "github_avatar_url")]
	#[builder(default, setter(into, strip_option(fallback = github_avatar_url_opt)))]
	pub github_avatar_url: Option<String>,
	#[welds(rename = "github_access_token")]
	#[builder(setter(into))]
	pub github_access_token: String,
	#[welds(rename = "email")]
	#[builder(default, setter(into, strip_option(fallback = email_opt)))]
	pub email: Option<String>,
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

fn before_create_user(model: &mut User) -> welds::errors::Result<()> {
	let now = chrono::Utc::now();
	model.created_at = now;
	model.updated_at = now;
	Ok(())
}

fn before_update_user(model: &mut User) -> welds::errors::Result<()> {
	model.updated_at = chrono::Utc::now();
	Ok(())
}
