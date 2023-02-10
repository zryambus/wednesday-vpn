use clokwerk::{AsyncScheduler, TimeUnits, Job, Interval::*};
use anyhow::anyhow;
use std::time::Duration;

pub fn run_collector() -> tokio::task::JoinHandle<()> {
    let mut scheduler = AsyncScheduler::new();
    scheduler.every(1.minute()).run(|| async {});
    tokio::spawn(async move {
        loop {
            scheduler.run_pending().await;
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    })
}
