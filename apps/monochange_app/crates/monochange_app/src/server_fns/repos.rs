//! Repository management server functions.

use leptos::server;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoInfo {
    pub id: i32,
    pub github_full_name: String,
    pub github_private: bool,
    pub plan_tier: String,
}

#[server]
pub async fn list_repos() -> Result<Vec<RepoInfo>, server_fn::ServerFnError> {
    Ok(vec![])
}

#[server]
pub async fn get_repo(full_name: String) -> Result<Option<RepoInfo>, server_fn::ServerFnError> {
    let _ = full_name;
    Ok(None)
}
