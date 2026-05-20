//! Release cadence modeling that can be unit-tested without GitHub or a repo.

use chrono::DateTime;
use chrono::Duration;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;

/// A positive duration stored in whole minutes for durable schedules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DurationMinutes(u32);

impl DurationMinutes {
	/// Create a new positive minute duration.
	pub fn new(minutes: u32) -> Option<Self> {
		(minutes > 0).then_some(Self(minutes))
	}

	/// Create a new positive minute duration, panicking on invalid constants.
	pub const fn new_unchecked(minutes: u32) -> Self {
		assert!(minutes > 0, "duration must be positive");
		Self(minutes)
	}

	/// Return the whole-minute value.
	pub const fn get(self) -> u32 {
		self.0
	}

	/// Convert to a chrono duration.
	pub fn to_chrono(self) -> Duration {
		Duration::minutes(i64::from(self.0))
	}
}

/// The next schedule cursor after one due occurrence is consumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NextOccurrence {
	/// Next time the schedule should produce a job.
	pub run_at: DateTime<Utc>,
	/// Zero-based batch index within the next release window.
	pub window_batch_index: u16,
}

/// Release cadence supported by the hosted app scheduler.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReleaseCadence {
	/// Run at a fixed interval.
	Interval { every: DurationMinutes },

	/// Run a bounded number of release batches, pause for a cooldown, then repeat.
	///
	/// This covers cadences like: release four packages every four hours, then
	/// wait 24 hours before opening the next release window.
	WindowedBatches {
		batch_count: u16,
		batch_spacing: DurationMinutes,
		cooldown: DurationMinutes,
	},
}

impl ReleaseCadence {
	/// A sensible default for early testing: one run every day.
	pub const fn daily() -> Self {
		Self::Interval {
			every: DurationMinutes::new_unchecked(24 * 60),
		}
	}

	/// The cadence described for staged multi-package releases.
	pub const fn four_batches_every_four_hours_then_daily() -> Self {
		Self::WindowedBatches {
			batch_count: 4,
			batch_spacing: DurationMinutes::new_unchecked(4 * 60),
			cooldown: DurationMinutes::new_unchecked(24 * 60),
		}
	}

	/// Calculate the next scheduled time after consuming one occurrence.
	pub fn next_after(&self, occurrence: DateTime<Utc>, window_batch_index: u16) -> NextOccurrence {
		match self {
			Self::Interval { every } => {
				NextOccurrence {
					run_at: occurrence + every.to_chrono(),
					window_batch_index: 0,
				}
			}
			Self::WindowedBatches {
				batch_count,
				batch_spacing,
				cooldown,
			} => {
				let batch_count = (*batch_count).max(1);
				let next_batch_index = window_batch_index.saturating_add(1);

				if next_batch_index >= batch_count {
					NextOccurrence {
						run_at: occurrence + cooldown.to_chrono(),
						window_batch_index: 0,
					}
				} else {
					NextOccurrence {
						run_at: occurrence + batch_spacing.to_chrono(),
						window_batch_index: next_batch_index,
					}
				}
			}
		}
	}

	/// Calculate upcoming occurrences from a starting cursor.
	pub fn occurrences(
		&self,
		first: DateTime<Utc>,
		window_batch_index: u16,
		count: usize,
	) -> Vec<DateTime<Utc>> {
		let mut current = first;
		let mut batch_index = window_batch_index;
		let mut occurrences = Vec::with_capacity(count);

		for _ in 0..count {
			occurrences.push(current);
			let next = self.next_after(current, batch_index);
			current = next.run_at;
			batch_index = next.window_batch_index;
		}

		occurrences
	}
}

impl Default for ReleaseCadence {
	fn default() -> Self {
		Self::daily()
	}
}
