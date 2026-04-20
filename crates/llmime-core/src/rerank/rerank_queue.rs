use std::cmp;

use anyhow::Result;
use tokio::sync::mpsc;

use super::{BoundaryEvent, Token};

#[derive(Debug, Clone, Copy)]
pub struct RerankConfig {
    pub window_size: usize,
    pub threshold: f32,
    pub channel_capacity: usize,
}

impl Default for RerankConfig {
    fn default() -> Self {
        Self {
            window_size: 5,
            threshold: 0.7,
            channel_capacity: 32,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RerankRequest {
    pub tokens: Vec<Token>,
    pub context_left: String,
    pub window_size: usize,
    pub threshold: f32,
}

/// 低確信度トークンを非同期再推論ワーカーに投入するキュー。
/// 打鍵スレッドをブロックしない (tokio::spawn + bounded channel)。
#[derive(Clone)]
pub struct RerankQueue {
    sender: mpsc::Sender<RerankRequest>,
    config: RerankConfig,
}

impl RerankQueue {
    pub fn new(config: RerankConfig) -> (Self, mpsc::Receiver<RerankRequest>) {
        let capacity = cmp::max(config.channel_capacity, 1);
        let (sender, receiver) = mpsc::channel(capacity);
        (Self { sender, config }, receiver)
    }

    pub async fn enqueue(&self, event: BoundaryEvent, buffer: &str) -> Result<()> {
        let mut targets: Vec<_> = event
            .recent_tokens
            .iter()
            .rev()
            .take(self.config.window_size)
            .filter(|t| t.confidence < self.config.threshold)
            .cloned()
            .collect();

        if targets.is_empty() {
            return Ok(());
        }
        targets.reverse();

        let request = RerankRequest {
            tokens: targets,
            context_left: buffer.to_string(),
            window_size: self.config.window_size,
            threshold: self.config.threshold,
        };

        let sender = self.sender.clone();
        tokio::spawn(async move {
            let _ = sender.try_send(request);
        });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{sleep, timeout, Duration};

    fn token(surface: &str, confidence: f32) -> Token {
        Token {
            surface: surface.to_string(),
            reading: surface.to_string(),
            pos: "noun".to_string(),
            pos_detail: "general".to_string(),
            start: 0,
            end: surface.chars().count(),
            confidence,
        }
    }

    fn event_with_tokens(tokens: Vec<Token>) -> BoundaryEvent {
        let new_token = tokens
            .last()
            .cloned()
            .unwrap_or_else(|| token("dummy", 1.0));
        BoundaryEvent {
            position: 0,
            new_token,
            timestamp: std::time::Instant::now(),
            recent_tokens: tokens,
        }
    }

    #[tokio::test]
    async fn rerank_queue_enqueues_only_low_confidence_tokens() {
        let (queue, mut rx) = RerankQueue::new(RerankConfig {
            window_size: 4,
            threshold: 0.7,
            channel_capacity: 32,
        });

        let event = event_with_tokens(vec![
            token("A", 0.95),
            token("B", 0.60),
            token("C", 0.10),
            token("D", 0.82),
            token("E", 0.40),
        ]);

        queue.enqueue(event, "left context").await.unwrap();

        let req = timeout(Duration::from_millis(100), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");

        let surfaces: Vec<_> = req.tokens.iter().map(|t| t.surface.as_str()).collect();
        assert_eq!(surfaces, vec!["B", "C", "E"]);
        assert_eq!(req.context_left, "left context");
        assert_eq!(req.window_size, 4);
        assert_eq!(req.threshold, 0.7);
    }

    #[tokio::test]
    async fn rerank_queue_does_not_block_keystroke_thread() {
        let (queue, _rx) = RerankQueue::new(RerankConfig {
            window_size: 5,
            threshold: 1.0,
            channel_capacity: 1,
        });

        let event = event_with_tokens(vec![token("x", 0.5)]);

        queue.enqueue(event.clone(), "buf").await.unwrap();
        sleep(Duration::from_millis(20)).await;

        let second = timeout(Duration::from_millis(10), queue.enqueue(event, "buf2")).await;
        assert!(
            second.is_ok(),
            "enqueue should return within 10ms even when channel is full"
        );
    }

    #[tokio::test]
    async fn rerank_queue_drops_on_bounded_channel_overflow() {
        let (queue, mut rx) = RerankQueue::new(RerankConfig {
            window_size: 5,
            threshold: 1.0,
            channel_capacity: 1,
        });

        let first = event_with_tokens(vec![token("first", 0.4)]);
        let second = event_with_tokens(vec![token("second", 0.3)]);

        queue.enqueue(first, "ctx").await.unwrap();
        sleep(Duration::from_millis(20)).await;
        queue.enqueue(second, "ctx").await.unwrap();
        sleep(Duration::from_millis(20)).await;

        let req = timeout(Duration::from_millis(100), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        assert_eq!(req.tokens[0].surface, "first");

        let next = timeout(Duration::from_millis(30), rx.recv()).await;
        assert!(
            next.is_err(),
            "second request should be dropped when bounded channel is full"
        );
    }
}
