//! Repository model.

use welds::WeldsModel;

#[derive(Debug, WeldsModel)]
#[welds(schema = "public", table = "repositories")]
#[welds(BelongsTo(installation, super::installation::Installation, "installation_id"))]
pub struct Repository {
    #[welds(primary_key)]
    #[welds(rename = "id")]
    pub id: i32,
    #[welds(rename = "installation_id")]
    pub installation_id: i32,
    #[welds(rename = "github_repo_id")]
    pub github_repo_id: i64,
    #[welds(rename = "github_full_name")]
    pub github_full_name: String,
    #[welds(rename = "github_private")]
    pub github_private: bool,
    #[welds(rename = "monochange_config_hash")]
    pub monochange_config_hash: Option<String>,
    #[welds(rename = "settings_json")]
    pub settings_json: Option<String>,
    #[welds(rename = "plan_tier")]
    pub plan_tier: String,
    #[welds(rename = "created_at")]
    pub created_at: chrono::DateTime<chrono::Utc>,
    #[welds(rename = "updated_at")]
    pub updated_at: chrono::DateTime<chrono::Utc>,
}
