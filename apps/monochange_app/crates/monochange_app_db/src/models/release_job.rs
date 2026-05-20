//! Durable release job model.

use typed_builder::TypedBuilder;
use welds::WeldsModel;

#[derive(Debug, Clone, WeldsModel, TypedBuilder)]
#[welds(table = "release_jobs")]
#[welds(BelongsTo(schedule, super::release_schedule::ReleaseSchedule, "schedule_id"))]
#[welds(BelongsTo(repository, super::repository::Repository, "repository_id"))]
#[welds(BeforeCreate(before_create_release_job))]
#[welds(BeforeUpdate(before_update_release_job))]
pub struct ReleaseJob {
	#[welds(primary_key)]
	#[welds(rename = "id")]
	#[builder(setter(into))]
	pub id: String,
	#[welds(rename = "schedule_id")]
	pub schedule_id: i32,
	#[welds(rename = "repository_id")]
	pub repository_id: i32,
	#[welds(rename = "kind")]
	#[builder(setter(into))]
	pub kind: String,
	#[welds(rename = "status")]
	#[builder(setter(into))]
	pub status: String,
	#[welds(rename = "run_after")]
	pub run_after: chrono::DateTime<chrono::Utc>,
	#[welds(rename = "scheduled_for")]
	pub scheduled_for: chrono::DateTime<chrono::Utc>,
	#[welds(rename = "attempts")]
	#[builder(default)]
	pub attempts: i32,
	#[welds(rename = "max_attempts")]
	#[builder(default = 5)]
	pub max_attempts: i32,
	#[welds(rename = "locked_by")]
	#[builder(default, setter(into, strip_option(fallback = locked_by_opt)))]
	pub locked_by: Option<String>,
	#[welds(rename = "locked_until")]
	#[builder(default, setter(into, strip_option(fallback = locked_until_opt)))]
	pub locked_until: Option<chrono::DateTime<chrono::Utc>>,
	#[welds(rename = "idempotency_key")]
	#[builder(setter(into))]
	pub idempotency_key: String,
	#[welds(rename = "payload_json")]
	#[builder(setter(into))]
	pub payload_json: String,
	#[welds(rename = "result_json")]
	#[builder(default, setter(into, strip_option(fallback = result_json_opt)))]
	pub result_json: Option<String>,
	#[welds(rename = "last_error")]
	#[builder(default, setter(into, strip_option(fallback = last_error_opt)))]
	pub last_error: Option<String>,
	#[welds(rename = "created_at")]
	#[builder(default = chrono::Utc::now())]
	pub created_at: chrono::DateTime<chrono::Utc>,
	#[welds(rename = "updated_at")]
	#[builder(default = chrono::Utc::now())]
	pub updated_at: chrono::DateTime<chrono::Utc>,
}

fn before_create_release_job(model: &mut ReleaseJob) -> welds::errors::Result<()> {
	let now = chrono::Utc::now();
	model.created_at = now;
	model.updated_at = now;
	Ok(())
}

fn before_update_release_job(model: &mut ReleaseJob) -> welds::errors::Result<()> {
	model.updated_at = chrono::Utc::now();
	Ok(())
}
