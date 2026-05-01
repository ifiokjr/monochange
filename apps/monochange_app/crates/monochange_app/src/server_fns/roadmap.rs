//! Roadmap server functions.

use leptos::server;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoadmapItem {
    pub id: i32,
    pub title: String,
    pub description: String,
    pub status: String,
    pub votes: i32,
}

#[server]
pub async fn list_roadmap(repo_id: i32) -> Result<Vec<RoadmapItem>, server_fn::ServerFnError> {
    let _ = repo_id;
    Ok(vec![])
}

#[server]
pub async fn vote_roadmap_item(item_id: i32) -> Result<i32, server_fn::ServerFnError> {
    let _ = item_id;
    Ok(0)
}
