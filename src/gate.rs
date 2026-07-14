//! 送信系 API の直列化 + レートリミット時の指数バックオフ再送。
//!
//! Misskey のレート制限はエンドポイント毎・インスタンス毎に異なるため
//! 固定間隔ではなく、送信を 1 本に直列化した上で `RATE_LIMIT_EXCEEDED`
//! (HTTP 429) を受けたときだけ指数バックオフで再送する。

use std::future::Future;
use std::time::Duration;

use notecli::error::NoteDeckError;

pub(crate) struct SendGate {
    lock: tokio::sync::Mutex<()>,
    base_delay: Duration,
    max_retries: u32,
}

impl SendGate {
    pub(crate) fn new() -> Self {
        Self::with_backoff(Duration::from_secs(2), 3)
    }

    fn with_backoff(base_delay: Duration, max_retries: u32) -> Self {
        Self {
            lock: tokio::sync::Mutex::new(()),
            base_delay,
            max_retries,
        }
    }

    /// 送信操作を直列化して実行する。レートリミット時は base_delay から
    /// 倍々で max_retries 回まで再送し、それ以外のエラーは即座に返す。
    pub(crate) async fn send<T, F, Fut>(&self, mut op: F) -> Result<T, NoteDeckError>
    where
        F: FnMut() -> Fut,
        Fut: Future<Output = Result<T, NoteDeckError>>,
    {
        let _guard = self.lock.lock().await;
        let mut delay = self.base_delay;
        let mut attempt = 0;
        loop {
            match op().await {
                Err(e) if is_rate_limited(&e) && attempt < self.max_retries => {
                    attempt += 1;
                    tracing::warn!(
                        attempt,
                        delay_ms = delay.as_millis() as u64,
                        "rate limited; backing off"
                    );
                    tokio::time::sleep(delay).await;
                    delay *= 2;
                }
                other => return other,
            }
        }
    }
}

fn is_rate_limited(e: &NoteDeckError) -> bool {
    // notecli の Api エラーは Misskey のエラーコードを message に畳み込む
    // (構造化 code フィールドは無い — 上流改善候補)
    matches!(
        e,
        NoteDeckError::Api { status, message, .. }
            if *status == 429 || message.contains("RATE_LIMIT_EXCEEDED")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn rate_limit_error() -> NoteDeckError {
        NoteDeckError::Api {
            endpoint: "notes/create".to_string(),
            status: 429,
            message: "notes/create: RATE_LIMIT_EXCEEDED: Rate limit exceeded".to_string(),
        }
    }

    fn other_error() -> NoteDeckError {
        NoteDeckError::Api {
            endpoint: "notes/create".to_string(),
            status: 400,
            message: "notes/create: NO_SUCH_NOTE".to_string(),
        }
    }

    #[test]
    fn detects_rate_limit() {
        assert!(is_rate_limited(&rate_limit_error()));
        assert!(!is_rate_limited(&other_error()));
        assert!(!is_rate_limited(&NoteDeckError::ConnectionClosed));
    }

    #[tokio::test(start_paused = true)]
    async fn retries_on_rate_limit_then_succeeds() {
        let gate = SendGate::with_backoff(Duration::from_millis(10), 3);
        let attempts = AtomicU32::new(0);
        let result = gate
            .send(|| {
                let n = attempts.fetch_add(1, Ordering::SeqCst);
                async move {
                    if n < 2 {
                        Err(rate_limit_error())
                    } else {
                        Ok("ok")
                    }
                }
            })
            .await;
        assert_eq!(result.unwrap(), "ok");
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn gives_up_after_max_retries() {
        let gate = SendGate::with_backoff(Duration::from_millis(10), 2);
        let attempts = AtomicU32::new(0);
        let result: Result<(), _> = gate
            .send(|| {
                attempts.fetch_add(1, Ordering::SeqCst);
                async { Err(rate_limit_error()) }
            })
            .await;
        assert!(is_rate_limited(&result.unwrap_err()));
        assert_eq!(attempts.load(Ordering::SeqCst), 3); // 初回 + 再送2回
    }

    #[tokio::test(start_paused = true)]
    async fn other_errors_fail_fast() {
        let gate = SendGate::with_backoff(Duration::from_millis(10), 3);
        let attempts = AtomicU32::new(0);
        let result: Result<(), _> = gate
            .send(|| {
                attempts.fetch_add(1, Ordering::SeqCst);
                async { Err(other_error()) }
            })
            .await;
        assert!(result.is_err());
        assert_eq!(attempts.load(Ordering::SeqCst), 1);
    }
}
