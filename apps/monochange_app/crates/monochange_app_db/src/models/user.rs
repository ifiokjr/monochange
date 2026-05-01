//! User model.

use welds::WeldsModel;

#[derive(Debug, WeldsModel)]
#[welds(schema = "public", table = "users")]
#[welds(HasMany(installations, super::installation::Installation, "user_id"))]
#[welds(HasMany(org_memberships, super::organization::OrganizationMember, "user_id"))]
pub struct User {
    #[welds(primary_key)]
    #[welds(rename = "id")]
    pub id: i32,
    #[welds(rename = "github_id")]
    pub github_id: i64,
    #[welds(rename = "github_login")]
    pub github_login: String,
    #[welds(rename = "github_avatar_url")]
    pub github_avatar_url: Option<String>,
    #[welds(rename = "github_access_token")]
    pub github_access_token: String,
    #[welds(rename = "email")]
    pub email: Option<String>,
    #[welds(rename = "plan_tier")]
    pub plan_tier: String,
    #[welds(rename = "created_at")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[welds(rename = "updated_at")]
    pub updated_at: chrono::DateTime<chrono::Utc>,
}
