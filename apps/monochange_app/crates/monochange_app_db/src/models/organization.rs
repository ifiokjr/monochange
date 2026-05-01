//! Organization model.

use welds::WeldsModel;

#[derive(Debug, WeldsModel)]
#[welds(schema = "public", table = "organizations")]
#[welds(HasMany(members, super::organization::OrganizationMember, "org_id"))]
pub struct Organization {
    #[welds(primary_key)]
    #[welds(rename = "id")]
    pub id: i32,
    #[welds(rename = "github_id")]
    pub github_id: i64,
    #[welds(rename = "github_login")]
    pub github_login: String,
    #[welds(rename = "github_avatar_url")]
    pub github_avatar_url: Option<String>,
    #[welds(rename = "created_at")]
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, WeldsModel)]
#[welds(schema = "public", table = "organization_members")]
#[welds(BelongsTo(user, super::user::User, "user_id"))]
#[welds(BelongsTo(org, super::organization::Organization, "org_id"))]
pub struct OrganizationMember {
    #[welds(primary_key)]
    #[welds(rename = "id")]
    pub id: i32,
    #[welds(rename = "user_id")]
    pub user_id: i32,
    #[welds(rename = "org_id")]
    pub org_id: i32,
    #[welds(rename = "role")]
    pub role: String,
    #[welds(rename = "created_at")]
    pub created_at: chrono::DateTime<chrono::Utc>,
}
