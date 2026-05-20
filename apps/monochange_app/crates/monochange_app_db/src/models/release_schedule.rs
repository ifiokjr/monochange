//! Durable release schedule model.

use typed_builder::TypedBuilder;
use welds::WeldsModel;

#[derive(Debug, Clone, WeldsModel, TypedBuilder)]
#[welds(table = "release_schedules")]
#[welds(BelongsTo(repository, super::repository::Repository, "repository_id"))]
#[welds(BeforeCreate(before_create_release_schedule))]
#[welds(BeforeUpdate(before_update_release_schedule))]
pub struct ReleaseSchedule {
	#[welds(primary_key)]
	#[welds(rename = "id")]
	#[builder(default)]
	pub id: i32,
	#[welds(rename = "repository_id")]
	pub repository_id: i32,
	#[welds(rename = "enabled")]
	#[builder(default = true)]
	pub enabled: bool,
	#[welds(rename = "cadence_json")]
	#[builder(setter(into))]
	pub cadence_json: String,
	#[welds(rename = "next_run_at")]
	pub next_run_at: chrono::DateTime<chrono::Utc>,
	#[welds(rename = "window_batch_index")]
	#[builder(default)]
	pub window_batch_index: i32,
	#[welds(rename = "last_enqueued_at")]
	#[builder(default, setter(into, strip_option(fallback = last_enqueued_at_opt)))]
	pub last_enqueued_at: Option<chrono::DateTime<chrono::Utc>>,
	#[welds(rename = "base_ref")]
	#[builder(default = "main".to_string(), setter(into))]
	pub base_ref: String,
	#[welds(rename = "requested_by_user_id")]
	#[builder(default, setter(into, strip_option(fallback = requested_by_user_id_opt)))]
	pub requested_by_user_id: Option<i32>,
	#[welds(rename = "created_at")]
	#[builder(default = chrono::Utc::now())]
	pub created_at: chrono::DateTime<chrono::Utc>,
	#[welds(rename = "updated_at")]
	#[builder(default = chrono::Utc::now())]
	pub updated_at: chrono::DateTime<chrono::Utc>,
}

fn before_create_release_schedule(model: &mut ReleaseSchedule) -> welds::errors::Result<()> {
	let now = chrono::Utc::now();
	model.created_at = now;
	model.updated_at = now;
	Ok(())
}

fn before_update_release_schedule(model: &mut ReleaseSchedule) -> welds::errors::Result<()> {
	model.updated_at = chrono::Utc::now();
	Ok(())
}
