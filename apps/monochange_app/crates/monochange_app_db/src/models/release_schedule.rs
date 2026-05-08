//! Durable release schedule model.

use welds::WeldsModel;

#[derive(Debug, WeldsModel)]
#[welds(schema = "public", table = "release_schedules")]
#[welds(BelongsTo(repository, super::repository::Repository, "repository_id"))]
pub struct ReleaseSchedule {
	#[welds(primary_key)]
	#[welds(rename = "id")]
	pub id: i32,
	#[welds(rename = "repository_id")]
	pub repository_id: i32,
	#[welds(rename = "enabled")]
	pub enabled: bool,
	#[welds(rename = "cadence_json")]
	pub cadence_json: String,
	#[welds(rename = "next_run_at")]
	pub next_run_at: chrono::DateTime<chrono::Utc>,
	#[welds(rename = "window_batch_index")]
	pub window_batch_index: i32,
	#[welds(rename = "last_enqueued_at")]
	pub last_enqueued_at: Option<chrono::DateTime<chrono::Utc>>,
	#[welds(rename = "base_ref")]
	pub base_ref: String,
	#[welds(rename = "requested_by_user_id")]
	pub requested_by_user_id: Option<i32>,
	#[welds(rename = "created_at")]
	pub created_at: chrono::DateTime<chrono::Utc>,
	#[welds(rename = "updated_at")]
	pub updated_at: chrono::DateTime<chrono::Utc>,
}
