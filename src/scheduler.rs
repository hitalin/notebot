//! cron スケジューラ。croner で次回発火を計算し、tokio task で sleep する
//! 素朴な実装。永続化しない (プロセス再起動で次周期から)。時刻はローカル
//! タイムゾーン (コンテナでは TZ 環境変数で制御)。

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use croner::Cron;

use crate::context::BotHandle;
use crate::error::Result;

pub(crate) type ScheduleHandler =
    Arc<dyn Fn(BotHandle) -> Pin<Box<dyn Future<Output = Result<()>> + Send>> + Send + Sync>;

pub(crate) struct Job {
    pub(crate) cron: Cron,
    /// 元のパターン文字列 (ログ用)
    pub(crate) source: String,
    pub(crate) handler: ScheduleHandler,
}

pub(crate) fn spawn_jobs(jobs: Vec<Job>, bot: &BotHandle) -> Vec<tokio::task::JoinHandle<()>> {
    jobs.into_iter()
        .map(|job| {
            let bot = bot.clone();
            tokio::spawn(async move {
                loop {
                    let now = chrono::Local::now();
                    let next = match job.cron.find_next_occurrence(&now, false) {
                        Ok(n) => n,
                        Err(e) => {
                            tracing::error!(pattern = job.source, error = %e, "no next occurrence; job stopped");
                            break;
                        }
                    };
                    let wait = (next - now).to_std().unwrap_or_default();
                    tokio::time::sleep(wait).await;
                    tracing::debug!(pattern = job.source, "running scheduled job");
                    if let Err(e) = (job.handler)(bot.clone()).await {
                        tracing::error!(pattern = job.source, error = %e, "scheduled job failed");
                    }
                }
            })
        })
        .collect()
}
