//! Organization model.

use typed_builder::TypedBuilder;
use welds::WeldsModel;

#[derive(Debug, Clone, WeldsModel, TypedBuilder)]
#[welds(table = "organizations")]
#[welds(HasMany(members, super::organization::OrganizationMember, "org_id"))]
#[welds(BeforeCreate(before_create_organization))]
#[welds(BeforeUpdate(before_update_organization))]
pub struct Organization {
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
	#[welds(rename = "created_at")]
	#[builder(default = chrono::Utc::now())]
	pub created_at: chrono::DateTime<chrono::Utc>,
	#[welds(rename = "updated_at")]
	#[builder(default = chrono::Utc::now())]
	pub updated_at: chrono::DateTime<chrono::Utc>,
}

fn before_create_organization(model: &mut Organization) -> welds::errors::Result<()> {
	let now = chrono::Utc::now();
	model.created_at = now;
	model.updated_at = now;
	Ok(())
}

fn before_update_organization(model: &mut Organization) -> welds::errors::Result<()> {
	model.updated_at = chrono::Utc::now();
	Ok(())
}

#[derive(Debug, Clone, WeldsModel, TypedBuilder)]
#[welds(table = "organization_members")]
#[welds(BelongsTo(user, super::user::User, "user_id"))]
#[welds(BelongsTo(org, super::organization::Organization, "org_id"))]
#[welds(BeforeCreate(before_create_organization_member))]
#[welds(BeforeUpdate(before_update_organization_member))]
pub struct OrganizationMember {
	#[welds(primary_key)]
	#[welds(rename = "id")]
	#[builder(default)]
	pub id: i32,
	#[welds(rename = "user_id")]
	pub user_id: i32,
	#[welds(rename = "org_id")]
	pub org_id: i32,
	#[welds(rename = "role")]
	#[builder(default = "member".to_string(), setter(into))]
	pub role: String,
	#[welds(rename = "created_at")]
	#[builder(default = chrono::Utc::now())]
	pub created_at: chrono::DateTime<chrono::Utc>,
	#[welds(rename = "updated_at")]
	#[builder(default = chrono::Utc::now())]
	pub updated_at: chrono::DateTime<chrono::Utc>,
}

fn before_create_organization_member(model: &mut OrganizationMember) -> welds::errors::Result<()> {
	let now = chrono::Utc::now();
	model.created_at = now;
	model.updated_at = now;
	Ok(())
}

fn before_update_organization_member(model: &mut OrganizationMember) -> welds::errors::Result<()> {
	model.updated_at = chrono::Utc::now();
	Ok(())
}
