//! Coordination leases preventing embedded and external Memory consumers
//! from operating in contradictory execution modes.

use std::str::FromStr;
use std::time::Duration;

use sqlx::{PgPool, Postgres, Transaction};
use thiserror::Error;
use uuid::Uuid;

const MIN_LEASE_SECONDS: u64 = 1;
const MAX_LEASE_SECONDS: u64 = 3600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryExecutionMode {
    Embedded,
    External,
}

impl MemoryExecutionMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Embedded => "embedded",
            Self::External => "external",
        }
    }
}

impl FromStr for MemoryExecutionMode {
    type Err = MemoryExecutionModeError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "embedded" => Ok(Self::Embedded),
            "external" => Ok(Self::External),
            _ => Err(MemoryExecutionModeError::InvalidMode),
        }
    }
}

#[derive(Debug, Error)]
pub enum MemoryExecutionModeError {
    #[error("memory execution mode must be 'embedded' or 'external'")]
    InvalidMode,
    #[error("memory execution lease duration must be between 1 and 3600 seconds")]
    InvalidLeaseDuration,
    #[error("memory execution mode is held by a live lease in another mode")]
    ModeConflict,
    #[error("memory execution lease is missing, expired, or fenced by a newer mode generation")]
    LeaseLost,
    #[error("memory execution mode storage failed: {0}")]
    Storage(#[from] sqlx::Error),
}

#[derive(Clone)]
pub struct MemoryExecutionModeStore {
    pool: PgPool,
}

impl MemoryExecutionModeStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Claim or replace this process's lease. The returned generation fences
    /// stale processes after all leases in a mode expire and another mode starts.
    pub async fn claim(
        &self,
        mode: MemoryExecutionMode,
        process_id: Uuid,
        lease_duration: Duration,
    ) -> Result<i64, MemoryExecutionModeError> {
        validate_duration(lease_duration)?;
        let mut tx = self.pool.begin().await?;
        ensure_guard(&mut tx).await?;
        let (active_mode, mut generation): (Option<String>, i64) = sqlx::query_as(
            "SELECT active_mode, generation FROM memory_execution_mode_guard WHERE singleton=TRUE FOR UPDATE",
        )
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query("DELETE FROM memory_execution_mode_leases WHERE lease_until <= NOW()")
            .execute(&mut *tx)
            .await?;
        let live_other_mode: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM memory_execution_mode_leases WHERE mode <> $1 AND lease_until > NOW())",
        )
        .bind(mode.as_str())
        .fetch_one(&mut *tx)
        .await?;
        if live_other_mode {
            return Err(MemoryExecutionModeError::ModeConflict);
        }
        if active_mode.as_deref() != Some(mode.as_str()) {
            generation += 1;
            sqlx::query(
                "UPDATE memory_execution_mode_guard SET active_mode=$1, generation=$2, updated_at=NOW() WHERE singleton=TRUE",
            )
            .bind(mode.as_str())
            .bind(generation)
            .execute(&mut *tx)
            .await?;
        }
        sqlx::query(
            "INSERT INTO memory_execution_mode_leases (process_id, mode, generation, lease_until) VALUES ($1, $2, $3, NOW() + ($4 * INTERVAL '1 second')) ON CONFLICT (process_id) DO UPDATE SET mode=EXCLUDED.mode, generation=EXCLUDED.generation, lease_until=EXCLUDED.lease_until, updated_at=NOW()",
        )
        .bind(process_id)
        .bind(mode.as_str())
        .bind(generation)
        .bind(lease_duration.as_secs() as i64)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(generation)
    }

    /// Renew only a current, unexpired lease. Expired processes must claim anew.
    pub async fn renew(
        &self,
        mode: MemoryExecutionMode,
        process_id: Uuid,
        generation: i64,
        lease_duration: Duration,
    ) -> Result<(), MemoryExecutionModeError> {
        validate_duration(lease_duration)?;
        let updated = sqlx::query(
            "UPDATE memory_execution_mode_leases l SET lease_until=NOW() + ($4 * INTERVAL '1 second'), updated_at=NOW() FROM memory_execution_mode_guard g WHERE l.process_id=$1 AND l.mode=$2 AND l.generation=$3 AND l.lease_until > NOW() AND g.singleton=TRUE AND g.active_mode=$2 AND g.generation=$3",
        )
        .bind(process_id)
        .bind(mode.as_str())
        .bind(generation)
        .bind(lease_duration.as_secs() as i64)
        .execute(&self.pool)
        .await?;
        if updated.rows_affected() == 1 {
            Ok(())
        } else {
            Err(MemoryExecutionModeError::LeaseLost)
        }
    }

    /// Fence a projection transaction while holding the mode guard row until
    /// commit, so an expired process cannot write after another mode starts.
    pub async fn fence_in_transaction(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        mode: MemoryExecutionMode,
        process_id: Uuid,
        generation: i64,
        lease_duration: Duration,
    ) -> Result<(), MemoryExecutionModeError> {
        validate_duration(lease_duration)?;
        let (active_mode, active_generation): (Option<String>, i64) = sqlx::query_as(
            "SELECT active_mode, generation FROM memory_execution_mode_guard WHERE singleton=TRUE FOR UPDATE",
        )
        .fetch_one(&mut **tx)
        .await?;
        if active_mode.as_deref() != Some(mode.as_str()) || active_generation != generation {
            return Err(MemoryExecutionModeError::LeaseLost);
        }
        let lease_is_live: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM memory_execution_mode_leases WHERE process_id=$1 AND mode=$2 AND generation=$3 AND lease_until>NOW())",
        )
        .bind(process_id)
        .bind(mode.as_str())
        .bind(generation)
        .fetch_one(&mut **tx)
        .await?;
        if lease_is_live {
            Ok(())
        } else {
            Err(MemoryExecutionModeError::LeaseLost)
        }
    }

    /// Release only the caller's matching lease; stale callers cannot remove a
    /// newer claim that reused a process identifier.
    pub async fn release(
        &self,
        mode: MemoryExecutionMode,
        process_id: Uuid,
        generation: i64,
    ) -> Result<(), MemoryExecutionModeError> {
        let mut tx = self.pool.begin().await?;
        ensure_guard(&mut tx).await?;
        let _: (Option<String>, i64) = sqlx::query_as(
            "SELECT active_mode, generation FROM memory_execution_mode_guard WHERE singleton=TRUE FOR UPDATE",
        )
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query(
            "DELETE FROM memory_execution_mode_leases WHERE process_id=$1 AND mode=$2 AND generation=$3",
        )
        .bind(process_id)
        .bind(mode.as_str())
        .bind(generation)
        .execute(&mut *tx)
        .await?;
        let any_live: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM memory_execution_mode_leases WHERE lease_until > NOW())",
        )
        .fetch_one(&mut *tx)
        .await?;
        if !any_live {
            sqlx::query(
                "UPDATE memory_execution_mode_guard SET active_mode=NULL, updated_at=NOW() WHERE singleton=TRUE AND generation=$1",
            )
            .bind(generation)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

fn validate_duration(duration: Duration) -> Result<(), MemoryExecutionModeError> {
    if !(MIN_LEASE_SECONDS..=MAX_LEASE_SECONDS).contains(&duration.as_secs())
        || duration.subsec_nanos() != 0
    {
        return Err(MemoryExecutionModeError::InvalidLeaseDuration);
    }
    Ok(())
}

async fn ensure_guard(tx: &mut Transaction<'_, Postgres>) -> Result<(), MemoryExecutionModeError> {
    sqlx::query(
        "INSERT INTO memory_execution_mode_guard (singleton) VALUES (TRUE) ON CONFLICT (singleton) DO NOTHING",
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_supported_modes() {
        assert_eq!(
            "embedded".parse::<MemoryExecutionMode>().unwrap(),
            MemoryExecutionMode::Embedded
        );
        assert_eq!(
            "external".parse::<MemoryExecutionMode>().unwrap(),
            MemoryExecutionMode::External
        );
        assert!("both".parse::<MemoryExecutionMode>().is_err());
        assert!("External".parse::<MemoryExecutionMode>().is_err());
    }

    #[test]
    fn lease_duration_is_bounded_and_whole_seconds() {
        assert!(validate_duration(Duration::from_secs(1)).is_ok());
        assert!(validate_duration(Duration::from_secs(3600)).is_ok());
        assert!(validate_duration(Duration::from_millis(999)).is_err());
        assert!(validate_duration(Duration::from_secs(3601)).is_err());
        assert!(validate_duration(Duration::from_millis(1500)).is_err());
    }

    #[tokio::test]
    #[ignore = "requires an isolated PostgreSQL database with migrations applied"]
    async fn database_leases_allow_same_mode_and_fence_mode_switches() {
        let database_url = std::env::var("DATABASE_URL").expect("set DATABASE_URL");
        let pool = PgPool::connect(&database_url).await.unwrap();
        let store = MemoryExecutionModeStore::new(pool.clone());
        let first = Uuid::new_v4();
        let second = Uuid::new_v4();
        let duration = Duration::from_secs(30);

        let generation = store
            .claim(MemoryExecutionMode::Embedded, first, duration)
            .await
            .unwrap();
        assert_eq!(
            store
                .claim(MemoryExecutionMode::Embedded, second, duration)
                .await
                .unwrap(),
            generation
        );
        let mut fenced_tx = pool.begin().await.unwrap();
        store
            .fence_in_transaction(
                &mut fenced_tx,
                MemoryExecutionMode::Embedded,
                first,
                generation,
                duration,
            )
            .await
            .unwrap();
        store
            .renew(MemoryExecutionMode::Embedded, first, generation, duration)
            .await
            .unwrap();
        let competing_store = store.clone();
        let mut switch = tokio::spawn(async move {
            competing_store
                .claim(MemoryExecutionMode::External, Uuid::new_v4(), duration)
                .await
        });
        assert!(
            tokio::time::timeout(Duration::from_millis(100), &mut switch)
                .await
                .is_err()
        );
        fenced_tx.commit().await.unwrap();
        assert!(matches!(
            switch.await.unwrap(),
            Err(MemoryExecutionModeError::ModeConflict)
        ));
        assert!(matches!(
            store
                .claim(MemoryExecutionMode::External, Uuid::new_v4(), duration)
                .await,
            Err(MemoryExecutionModeError::ModeConflict)
        ));
        store
            .renew(MemoryExecutionMode::Embedded, first, generation, duration)
            .await
            .unwrap();
        store
            .release(MemoryExecutionMode::Embedded, first, generation)
            .await
            .unwrap();
        store
            .release(MemoryExecutionMode::Embedded, second, generation)
            .await
            .unwrap();

        let next_generation = store
            .claim(MemoryExecutionMode::External, Uuid::new_v4(), duration)
            .await
            .unwrap();
        assert!(next_generation > generation);
        sqlx::query("DELETE FROM memory_execution_mode_leases WHERE generation=$1")
            .bind(next_generation)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE memory_execution_mode_guard SET active_mode=NULL WHERE singleton=TRUE AND generation=$1")
            .bind(next_generation)
            .execute(&pool)
            .await
            .unwrap();

        let stale_process = Uuid::new_v4();
        let stale_generation = store
            .claim(
                MemoryExecutionMode::Embedded,
                stale_process,
                Duration::from_secs(1),
            )
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(1_100)).await;
        let replacement_generation = store
            .claim(
                MemoryExecutionMode::External,
                Uuid::new_v4(),
                Duration::from_secs(30),
            )
            .await
            .unwrap();
        assert!(replacement_generation > stale_generation);
        assert!(matches!(
            store
                .renew(
                    MemoryExecutionMode::Embedded,
                    stale_process,
                    stale_generation,
                    duration,
                )
                .await,
            Err(MemoryExecutionModeError::LeaseLost)
        ));
        sqlx::query("DELETE FROM memory_execution_mode_leases WHERE generation=$1")
            .bind(replacement_generation)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE memory_execution_mode_guard SET active_mode=NULL WHERE singleton=TRUE AND generation=$1")
            .bind(replacement_generation)
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
    }
}
