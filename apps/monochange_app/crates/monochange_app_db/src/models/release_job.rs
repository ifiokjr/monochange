//! Durable release job model.

use welds::WeldsModel;

#[derive(Debug, WeldsModel)]
#[welds(schema = "public", table = "release_jobs")]
#[welds(BelongsTo(schedule, super::release_schedule::ReleaseSchedule, "schedule_id"))]
#[welds(BelongsTo(repository, super::repository::Repository, "repository_id"))]
pub struct ReleaseJob {
	#[welds(primary_key)]
	#[welds(rename = "id")]
	pub id: String,
	#[welds(rename = "schedule_id")]
	pub schedule_id: i32,
	#[welds(rename = "repository_id")]
	pub repository_id: i32,
	#[welds(rename = "kind")]
	pub kind: String,
	#[welds(rename = "status")]
	pub status: String,
	#[welds(rename = "run_after")]
	pub run_after: chrono::DateTime<chrono::Utc>,
	#[welds(rename = "scheduled_for")]
	pub scheduled_for: chrono::DateTime<chrono::Utc>,
	#[welds(rename = "attempts")]
	pub attempts: i32,
	#[welds(rename = "max_attempts")]
	pub max_attempts: i32,
	#[welds(rename = "locked_by")]
	pub locked_by: Option<String>,
	#[welds(rename = "locked_until")]
	pub locked_until: Option<chrono::DateTime<chrono::Utc>>,
	#[welds(rename = "idempotency_key")]
	pub idempotency_key: String,
	#[welds(rename = "payload_json")]
	pub payload_json: String,
	#[welds(rename = "result_json")]
	pub result_json: Option<String>,
	#[welds(rename = "last_error")]
	pub last_error: Option<String>,
	#[welds(rename = "created_at")]
	pub created_at: chrono::DateTime<chrono::Utc>,
	#[welds(rename = "updated_at")]
	pub updated_at: chrono::DateTime<chrono::Utc>,
}
