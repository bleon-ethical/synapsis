use tokio::time::interval;

pub struct SessionCleanupJob {
    db: Arc<crate::infrastructure::database::Database>,
    interval_secs: u64,
}
use std::sync::Arc;

impl SessionCleanupJob {
    pub fn new(db: Arc<crate::infrastructure::database::Database>) -> Self {
        Self {
            db,
            interval_secs: 3600,
        }
    }

    pub fn with_interval(
        db: Arc<crate::infrastructure::database::Database>,
        interval_secs: u64,
    ) -> Self {
        Self { db, interval_secs }
    }

    pub async fn start(&self) {
        let mut interval_timer = interval(std::time::Duration::from_secs(self.interval_secs));

        eprintln!(
            "[SessionCleanup] Started - running every {} seconds",
            self.interval_secs
        );

        loop {
            interval_timer.tick().await;
            self.run_cleanup();
        }
    }

    pub fn run_cleanup(&self) -> usize {
        let threshold = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64)
            - 3600;

        match self.db.cleanup_stale_sessions(threshold) {
            Ok(count) => {
                if count > 0 {
                    eprintln!(
                        "[SessionCleanup] Cleaned {} stale sessions (threshold: {}s)",
                        count, threshold
                    );
                }
                count
            }
            Err(e) => {
                eprintln!("[SessionCleanup] Error: {}", e);
                0
            }
        }
    }
}

pub fn start_cleanup_job_background(db: Arc<crate::infrastructure::database::Database>) {
    let cleanup = SessionCleanupJob::new(db);

    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(cleanup.start());
    });
}
