//! PostgreSQL adapter for the durable release job store.

use async_trait::async_trait;
use chrono::DateTime;
use chrono::Duration;
use chrono::Utc;
use serde::Serialize;
use serde::de::DeserializeOwned;
use sqlx::PgPool;
use uuid::Uuid;

use crate::AutomationError;
use crate::JobResult;
use crate::JobStatus;
use crate::ReleaseJob;
use crate::ReleaseJobKind;
use crate::ReleaseJobPayload;
use crate::ReleaseJobStore;
use crate::ReleaseRepository;
use crate::ReleaseSchedule;

/// PostgreSQL implementation of [`ReleaseJobStore`].
#[derive(Debug, Clone)]
pub struct PostgresReleaseJobStore {
	pool: PgPool,
	schema: String,
}

impl PostgresReleaseJobStore {
	/// Create a store using the `public` schema.
	pub fn new(pool: PgPool) -> Self {
		Self {
			pool,
			schema: "public".to_string(),
		}
	}

	/// Create a store using a specific schema.
	///
	/// This is primarily useful for isolated integration tests.
	pub fn with_schema(pool: PgPool, schema: impl Into<String>) -> Result<Self, AutomationError> {
		let schema = schema.into();
		validate_identifier(&schema)?;
		Ok(Self { pool, schema })
	}

	fn table(&self, name: &str) -> String {
		format!("{}.{}", self.schema, name)
	}
}

#[async_trait]
impl ReleaseJobStore for PostgresReleaseJobStore {
	async fn enqueue_due_schedules(&self, now: DateTime<Utc>) -> Result<usize, AutomationError> {
		let mut tx = self.pool.begin().await.map_err(store_error)?;
		let schedules = sqlx::query_as::<_, ScheduleRow>(&format!(
			"SELECT s.id, s.repository_id, s.enabled, s.cadence_json, s.next_run_at, \
			 s.window_batch_index, s.last_enqueued_at, s.base_ref, s.requested_by_user_id, \
			 r.github_repo_id, r.github_full_name, i.github_installation_id \
			 FROM {} s \
			 JOIN {} r ON r.id = s.repository_id \
			 JOIN {} i ON i.id = r.installation_id \
			 WHERE s.enabled = TRUE AND s.next_run_at <= $1 \
			 ORDER BY s.next_run_at, s.id \
			 FOR UPDATE SKIP LOCKED",
			self.table("release_schedules"),
			self.table("repositories"),
			self.table("installations"),
		))
		.bind(now)
		.fetch_all(&mut *tx)
		.await
		.map_err(store_error)?;

		let mut enqueued = 0;
		for row in schedules {
			let mut schedule = row.try_into_schedule()?;
			let job = ReleaseJob::from_schedule(&schedule, Uuid::new_v4());
			let inserted = insert_job(&mut tx, &self.table("release_jobs"), &job).await?;
			if inserted {
				enqueued += 1;
			}

			schedule.advance_after_enqueue();
			sqlx::query(&format!(
				"UPDATE {} \
				 SET next_run_at = $1, window_batch_index = $2, last_enqueued_at = $3, updated_at = $4 \
				 WHERE id = $5",
				self.table("release_schedules"),
			))
			.bind(schedule.next_run_at)
			.bind(i32::from(schedule.window_batch_index))
			.bind(schedule.last_enqueued_at)
			.bind(now)
			.bind(schedule.id)
			.execute(&mut *tx)
			.await
			.map_err(store_error)?;
		}

		tx.commit().await.map_err(store_error)?;
		Ok(enqueued)
	}

	async fn claim_next_job(
		&self,
		worker_id: &str,
		now: DateTime<Utc>,
		lock_for: Duration,
	) -> Result<Option<ReleaseJob>, AutomationError> {
		let mut tx = self.pool.begin().await.map_err(store_error)?;
		let queued = enum_to_string(&JobStatus::Queued)?;
		let retryable = enum_to_string(&JobStatus::Retryable)?;
		let running = enum_to_string(&JobStatus::Running)?;
		let Some(row) = sqlx::query_as::<_, JobRow>(&format!(
			"SELECT id, schedule_id, kind, status, run_after, scheduled_for, attempts, max_attempts, \
			 locked_by, locked_until, idempotency_key, payload_json, result_json, last_error \
			 FROM {} \
			 WHERE (((status = $1 OR status = $2) AND run_after <= $3) \
			 OR (status = $4 AND locked_until IS NOT NULL AND locked_until <= $3)) \
			 ORDER BY run_after, id \
			 LIMIT 1 \
			 FOR UPDATE SKIP LOCKED",
			self.table("release_jobs"),
		))
		.bind(queued)
		.bind(retryable)
		.bind(now)
		.bind(running)
		.fetch_optional(&mut *tx)
		.await
		.map_err(store_error)?
		else {
			tx.commit().await.map_err(store_error)?;
			return Ok(None);
		};

		let locked_until = now + lock_for;
		let running = enum_to_string(&JobStatus::Running)?;
		let row = sqlx::query_as::<_, JobRow>(&format!(
			"UPDATE {} \
			 SET status = $1, locked_by = $2, locked_until = $3, attempts = attempts + 1, updated_at = $4 \
			 WHERE id = $5 \
			 RETURNING id, schedule_id, kind, status, run_after, scheduled_for, attempts, max_attempts, \
			 locked_by, locked_until, idempotency_key, payload_json, result_json, last_error",
			self.table("release_jobs"),
		))
		.bind(running)
		.bind(worker_id)
		.bind(locked_until)
		.bind(now)
		.bind(row.id)
		.fetch_one(&mut *tx)
		.await
		.map_err(store_error)?;

		tx.commit().await.map_err(store_error)?;
		Ok(Some(row.try_into_job()?))
	}

	async fn mark_succeeded(&self, job_id: Uuid, result: JobResult) -> Result<(), AutomationError> {
		let status = enum_to_string(&JobStatus::Succeeded)?;
		let result_json = to_json(&result)?;
		sqlx::query(&format!(
			"UPDATE {} \
			 SET status = $1, result_json = $2, locked_by = NULL, locked_until = NULL, \
			 last_error = NULL, updated_at = $3 \
			 WHERE id = $4",
			self.table("release_jobs"),
		))
		.bind(status)
		.bind(result_json)
		.bind(Utc::now())
		.bind(job_id.to_string())
		.execute(&self.pool)
		.await
		.map_err(store_error)?;
		Ok(())
	}

	async fn mark_retryable(
		&self,
		job_id: Uuid,
		error: String,
		next_run_at: DateTime<Utc>,
	) -> Result<(), AutomationError> {
		let status = enum_to_string(&JobStatus::Retryable)?;
		sqlx::query(&format!(
			"UPDATE {} \
			 SET status = $1, run_after = $2, locked_by = NULL, locked_until = NULL, \
			 last_error = $3, updated_at = $4 \
			 WHERE id = $5",
			self.table("release_jobs"),
		))
		.bind(status)
		.bind(next_run_at)
		.bind(error)
		.bind(Utc::now())
		.bind(job_id.to_string())
		.execute(&self.pool)
		.await
		.map_err(store_error)?;
		Ok(())
	}

	async fn mark_dead(&self, job_id: Uuid, error: String) -> Result<(), AutomationError> {
		let status = enum_to_string(&JobStatus::Dead)?;
		sqlx::query(&format!(
			"UPDATE {} \
			 SET status = $1, locked_by = NULL, locked_until = NULL, last_error = $2, updated_at = $3 \
			 WHERE id = $4",
			self.table("release_jobs"),
		))
		.bind(status)
		.bind(error)
		.bind(Utc::now())
		.bind(job_id.to_string())
		.execute(&self.pool)
		.await
		.map_err(store_error)?;
		Ok(())
	}
}

async fn insert_job(
	tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
	table: &str,
	job: &ReleaseJob,
) -> Result<bool, AutomationError> {
	let kind = enum_to_string(&job.kind)?;
	let status = enum_to_string(&job.status)?;
	let payload_json = to_json(&job.payload)?;
	let result = sqlx::query(&format!(
		"INSERT INTO {table} \
		 (id, schedule_id, repository_id, kind, status, run_after, scheduled_for, attempts, max_attempts, \
		 locked_by, locked_until, idempotency_key, payload_json, created_at, updated_at) \
		 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $14) \
		 ON CONFLICT (idempotency_key) DO NOTHING",
	))
	.bind(job.id.to_string())
	.bind(job.schedule_id)
	.bind(job.payload.repository.db_id)
	.bind(kind)
	.bind(status)
	.bind(job.run_after)
	.bind(job.scheduled_for)
	.bind(i32::from(job.attempts))
	.bind(i32::from(job.max_attempts))
	.bind(&job.locked_by)
	.bind(job.locked_until)
	.bind(&job.idempotency_key)
	.bind(payload_json)
	.bind(Utc::now())
	.execute(&mut **tx)
	.await
	.map_err(store_error)?;

	Ok(result.rows_affected() > 0)
}

#[derive(sqlx::FromRow)]
struct ScheduleRow {
	id: i32,
	repository_id: i32,
	enabled: bool,
	cadence_json: String,
	next_run_at: DateTime<Utc>,
	window_batch_index: i32,
	last_enqueued_at: Option<DateTime<Utc>>,
	base_ref: String,
	requested_by_user_id: Option<i32>,
	github_repo_id: i64,
	github_full_name: String,
	github_installation_id: i64,
}

impl ScheduleRow {
	fn try_into_schedule(self) -> Result<ReleaseSchedule, AutomationError> {
		Ok(ReleaseSchedule {
			id: self.id,
			repository: ReleaseRepository {
				db_id: self.repository_id,
				github_repo_id: self.github_repo_id,
				full_name: self.github_full_name,
				default_branch: self.base_ref.clone(),
				github_installation_id: self.github_installation_id,
			},
			enabled: self.enabled,
			cadence: from_json(&self.cadence_json)?,
			next_run_at: self.next_run_at,
			window_batch_index: u16::try_from(self.window_batch_index)
				.map_err(|_| AutomationError::store("invalid schedule batch index"))?,
			last_enqueued_at: self.last_enqueued_at,
			base_ref: self.base_ref,
			requested_by_user_id: self.requested_by_user_id,
		})
	}
}

#[derive(sqlx::FromRow)]
struct JobRow {
	id: String,
	schedule_id: i32,
	kind: String,
	status: String,
	run_after: DateTime<Utc>,
	scheduled_for: DateTime<Utc>,
	attempts: i32,
	max_attempts: i32,
	locked_by: Option<String>,
	locked_until: Option<DateTime<Utc>>,
	idempotency_key: String,
	payload_json: String,
	#[allow(dead_code)]
	result_json: Option<String>,
	last_error: Option<String>,
}

impl JobRow {
	fn try_into_job(self) -> Result<ReleaseJob, AutomationError> {
		Ok(ReleaseJob {
			id: Uuid::parse_str(&self.id).map_err(store_error)?,
			schedule_id: self.schedule_id,
			kind: enum_from_string::<ReleaseJobKind>(&self.kind)?,
			status: enum_from_string::<JobStatus>(&self.status)?,
			run_after: self.run_after,
			scheduled_for: self.scheduled_for,
			attempts: u16::try_from(self.attempts)
				.map_err(|_| AutomationError::store("invalid job attempt count"))?,
			max_attempts: u16::try_from(self.max_attempts)
				.map_err(|_| AutomationError::store("invalid job max attempt count"))?,
			locked_by: self.locked_by,
			locked_until: self.locked_until,
			idempotency_key: self.idempotency_key,
			payload: from_json::<ReleaseJobPayload>(&self.payload_json)?,
			last_error: self.last_error,
		})
	}
}

fn validate_identifier(identifier: &str) -> Result<(), AutomationError> {
	let valid = !identifier.is_empty()
		&& identifier
			.chars()
			.all(|ch| ch == '_' || ch.is_ascii_alphanumeric());
	if valid {
		Ok(())
	} else {
		Err(AutomationError::store(format!(
			"invalid postgres identifier {identifier:?}",
		)))
	}
}

fn to_json<T>(value: &T) -> Result<String, AutomationError>
where
	T: Serialize,
{
	serde_json::to_string(value).map_err(store_error)
}

fn from_json<T>(value: &str) -> Result<T, AutomationError>
where
	T: DeserializeOwned,
{
	serde_json::from_str(value).map_err(store_error)
}

fn enum_to_string<T>(value: &T) -> Result<String, AutomationError>
where
	T: Serialize,
{
	match serde_json::to_value(value).map_err(store_error)? {
		serde_json::Value::String(value) => Ok(value),
		_ => Err(AutomationError::store("expected string enum serialization")),
	}
}

fn enum_from_string<T>(value: &str) -> Result<T, AutomationError>
where
	T: DeserializeOwned,
{
	serde_json::from_value(serde_json::Value::String(value.to_string())).map_err(store_error)
}

fn store_error(error: impl std::fmt::Display) -> AutomationError {
	AutomationError::store(error.to_string())
}

#[cfg(test)]
mod tests {
	use chrono::TimeZone;
	use sqlx::Row;
	use sqlx::postgres::PgPoolOptions;

	use super::*;
	use crate::DurationMinutes;
	use crate::ReleaseCadence;

	#[tokio::test]
	async fn postgres_store_runs_durable_job_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
		if std::env::var("MONOCHANGE_APP_RUN_DB_TESTS").as_deref() != Ok("1") {
			eprintln!("skipping postgres store test; set MONOCHANGE_APP_RUN_DB_TESTS=1");
			return Ok(());
		}

		let database_url = std::env::var("MONOCHANGE_APP_AUTOMATION_DATABASE_URL")
			.or_else(|_| std::env::var("DATABASE_URL"))?;
		let pool = PgPoolOptions::new()
			.max_connections(2)
			.connect(&database_url)
			.await?;
		let schema = format!("mc_auto_test_{}", Uuid::new_v4().simple());
		let result = async {
			create_test_schema(&pool, &schema).await?;
			run_lifecycle_assertions(&pool, &schema).await
		}
		.await;
		let cleanup = sqlx::query(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE"))
			.execute(&pool)
			.await;
		cleanup?;
		result
	}

	async fn run_lifecycle_assertions(
		pool: &PgPool,
		schema: &str,
	) -> Result<(), Box<dyn std::error::Error>> {
		let now = Utc.with_ymd_and_hms(2026, 5, 7, 12, 0, 0).unwrap();
		let repository_id = seed_repository(pool, schema).await?;
		let cadence = ReleaseCadence::Interval {
			every: DurationMinutes::new_unchecked(60),
		};
		let cadence_json = serde_json::to_string(&cadence)?;
		sqlx::query(&format!(
			"INSERT INTO {schema}.release_schedules \
			 (repository_id, enabled, cadence_json, next_run_at, base_ref, requested_by_user_id) \
			 VALUES ($1, TRUE, $2, $3, 'main', NULL)",
		))
		.bind(repository_id)
		.bind(cadence_json)
		.bind(now)
		.execute(pool)
		.await?;

		let store = PostgresReleaseJobStore::with_schema(pool.clone(), schema)?;
		assert_eq!(store.enqueue_due_schedules(now).await?, 1);
		assert_eq!(store.enqueue_due_schedules(now).await?, 0);

		let next_run_at: DateTime<Utc> = sqlx::query_scalar(&format!(
			"SELECT next_run_at FROM {schema}.release_schedules LIMIT 1",
		))
		.fetch_one(pool)
		.await?;
		assert_eq!(next_run_at, now + Duration::hours(1));

		let job = store
			.claim_next_job("worker-a", now, Duration::minutes(15))
			.await?
			.expect("queued job should be claimable");
		assert_eq!(job.status, JobStatus::Running);
		assert_eq!(job.attempts, 1);
		assert_eq!(job.locked_by.as_deref(), Some("worker-a"));
		assert_eq!(job.payload.repository.full_name, "monochange/demo");

		store
			.mark_retryable(
				job.id,
				"temporary github outage".to_string(),
				now + Duration::minutes(5),
			)
			.await?;
		assert_eq!(job_status(pool, schema, job.id).await?, "retryable");

		let retry_job = store
			.claim_next_job(
				"worker-a",
				now + Duration::minutes(5),
				Duration::minutes(15),
			)
			.await?
			.expect("retryable job should become claimable");
		assert_eq!(retry_job.attempts, 2);

		store
			.mark_succeeded(
				retry_job.id,
				JobResult {
					summary: "workflow dispatched".to_string(),
					external_id: Some("workflow-run-456".to_string()),
					url: Some("https://github.com/monochange/demo/actions/runs/456".to_string()),
				},
			)
			.await?;
		assert_eq!(job_status(pool, schema, retry_job.id).await?, "succeeded");
		Ok(())
	}

	async fn create_test_schema(pool: &PgPool, schema: &str) -> Result<(), sqlx::Error> {
		sqlx::query(&format!("CREATE SCHEMA {schema}"))
			.execute(pool)
			.await?;
		sqlx::raw_sql(&format!(
			r#"
            CREATE TABLE {schema}.users (
                id SERIAL PRIMARY KEY,
                github_id BIGINT NOT NULL UNIQUE,
                github_login VARCHAR(255) NOT NULL,
                github_access_token TEXT NOT NULL
            );

            CREATE TABLE {schema}.installations (
                id SERIAL PRIMARY KEY,
                user_id INTEGER NOT NULL REFERENCES {schema}.users(id) ON DELETE CASCADE,
                github_installation_id BIGINT NOT NULL UNIQUE,
                github_account_login VARCHAR(255) NOT NULL,
                github_account_type VARCHAR(50) NOT NULL,
                target_type VARCHAR(50) NOT NULL DEFAULT 'selected'
            );

            CREATE TABLE {schema}.repositories (
                id SERIAL PRIMARY KEY,
                installation_id INTEGER NOT NULL REFERENCES {schema}.installations(id) ON DELETE CASCADE,
                github_repo_id BIGINT NOT NULL UNIQUE,
                github_full_name VARCHAR(512) NOT NULL,
                github_private BOOLEAN NOT NULL DEFAULT false
            );

            CREATE TABLE {schema}.release_schedules (
                id SERIAL PRIMARY KEY,
                repository_id INTEGER NOT NULL REFERENCES {schema}.repositories(id) ON DELETE CASCADE,
                enabled BOOLEAN NOT NULL DEFAULT true,
                cadence_json TEXT NOT NULL,
                next_run_at TIMESTAMPTZ NOT NULL,
                window_batch_index INTEGER NOT NULL DEFAULT 0,
                last_enqueued_at TIMESTAMPTZ,
                base_ref VARCHAR(255) NOT NULL DEFAULT 'main',
                requested_by_user_id INTEGER REFERENCES {schema}.users(id) ON DELETE SET NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );

            CREATE TABLE {schema}.release_jobs (
                id VARCHAR(36) PRIMARY KEY,
                schedule_id INTEGER NOT NULL REFERENCES {schema}.release_schedules(id) ON DELETE CASCADE,
                repository_id INTEGER NOT NULL REFERENCES {schema}.repositories(id) ON DELETE CASCADE,
                kind VARCHAR(64) NOT NULL,
                status VARCHAR(32) NOT NULL,
                run_after TIMESTAMPTZ NOT NULL,
                scheduled_for TIMESTAMPTZ NOT NULL,
                attempts INTEGER NOT NULL DEFAULT 0,
                max_attempts INTEGER NOT NULL DEFAULT 5,
                locked_by VARCHAR(255),
                locked_until TIMESTAMPTZ,
                idempotency_key VARCHAR(255) NOT NULL UNIQUE,
                payload_json TEXT NOT NULL,
                result_json TEXT,
                last_error TEXT,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );
        "#,
		))
		.execute(pool)
		.await?;
		Ok(())
	}

	async fn seed_repository(pool: &PgPool, schema: &str) -> Result<i32, sqlx::Error> {
		let user_id: i32 = sqlx::query_scalar(&format!(
			"INSERT INTO {schema}.users (github_id, github_login, github_access_token) \
			 VALUES (1, 'octocat', 'oauth-token') RETURNING id",
		))
		.fetch_one(pool)
		.await?;
		let installation_id: i32 = sqlx::query_scalar(&format!(
			"INSERT INTO {schema}.installations \
			 (user_id, github_installation_id, github_account_login, github_account_type) \
			 VALUES ($1, 99, 'monochange', 'Organization') RETURNING id",
		))
		.bind(user_id)
		.fetch_one(pool)
		.await?;
		sqlx::query_scalar(&format!(
			"INSERT INTO {schema}.repositories \
			 (installation_id, github_repo_id, github_full_name, github_private) \
			 VALUES ($1, 123, 'monochange/demo', false) RETURNING id",
		))
		.bind(installation_id)
		.fetch_one(pool)
		.await
	}

	async fn job_status(pool: &PgPool, schema: &str, job_id: Uuid) -> Result<String, sqlx::Error> {
		sqlx::query(&format!(
			"SELECT status FROM {schema}.release_jobs WHERE id = $1",
		))
		.bind(job_id.to_string())
		.fetch_one(pool)
		.await
		.map(|row| row.get("status"))
	}
}
