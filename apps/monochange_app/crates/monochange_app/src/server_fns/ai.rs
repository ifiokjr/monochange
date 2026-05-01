//! AI server functions.

use leptos::server;

#[server]
pub async fn ai_analyze_issue(
    repo_full_name: String,
    issue_number: i64,
) -> Result<String, server_fn::ServerFnError> {
    let _ = (repo_full_name, issue_number);
    Ok("AI analysis not yet implemented".to_string())
}
