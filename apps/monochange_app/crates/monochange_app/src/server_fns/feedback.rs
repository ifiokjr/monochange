//! Feedback server functions.

use leptos::server;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackSubmission {
    pub email: Option<String>,
    pub feedback: String,
    pub category: Option<String>,
}

#[server]
pub async fn submit_feedback(
    repo_slug: String,
    form_slug: String,
    submission: FeedbackSubmission,
) -> Result<(), server_fn::ServerFnError> {
    let _ = (repo_slug, form_slug, submission);
    Ok(())
}
