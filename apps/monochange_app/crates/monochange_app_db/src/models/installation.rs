//! Installation model — GitHub App installations.

use welds::WeldsModel;

#[derive(Debug, WeldsModel)]
#[welds(schema = "public", table = "installations")]
#[welds(BelongsTo(user, super::user::User, "user_id"))]
#[welds(HasMany(repos, super::repository::Repository, "installation_id"))]
pub struct Installation {
    #[welds(primary_key)]
    #[welds(rename = "id")]
    pub id: i32,
    #[welds(rename = "user_id")]
    pub user_id: i32,
    #[welds(rename = "github_installation_id")]
    pub github_installation_id: i64,
    #[welds(rename = "github_account_login")]
    pub github_account_login: String,
    #[welds(rename = "github_account_type")]
    pub github_account_type: String,
    #[welds(rename = "target_type")]
    pub target_type: String,
    #[welds(rename = "created_at")]
    pub created_at: chrono::DateTime<chrono::Utc>,
}
