use std::cmp;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::Result;
use tokio::sync::{mpsc, Mutex};

use super::{BoundaryEvent, Token};

#[derive(Debug, Clone, Copy)]
pub struct RerankConfig {
    pub window_size: usize,
    pub threshold: f32,
    pub channel_capacity: usize,
    pub batch_window_ms: u64,
    pub coalesce_char_gap: usize,
}

impl Default for RerankConfig {
    fn default() -> Self {
        Self {
            window_size: 5,
            threshold: 0.7,
            channel_capacity: 32,
            batch_window_ms: 40,
            coalesce_char_gap: 6,
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

#[derive(Debug)]
struct PendingBatch {
    request: RerankRequest,
    generation: u64,
    last_update: Instant,
}

#[derive(Debug, Default)]
struct CoalesceState {
    pending: Option<PendingBatch>,
    next_generation: u64,
}

/// 低確信度トークンを非同期再推論ワーカーに投入するキュー。
/// 打鍵スレッドをブロックしない (tokio::spawn + bounded channel)。
#[derive(Clone)]
pub struct RerankQueue {
    sender: mpsc::Sender<RerankRequest>,
    config: RerankConfig,
    state: Arc<Mutex<CoalesceState>>,
}

impl RerankQueue {
    pub fn new(config: RerankConfig) -> (Self, mpsc::Receiver<RerankRequest>) {
        let capacity = cmp::max(config.channel_capacity, 1);
        let (sender, receiver) = mpsc::channel(capacity);
        (
            Self {
                sender,
                config,
                state: Arc::new(Mutex::new(CoalesceState::default())),
            },
            receiver,
        )
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

        let (batch_gen, flush_now) = {
            let mut state = self.state.lock().await;
            let now = Instant::now();

            if let Some(pending) = state.pending.as_mut() {
                if can_coalesce(&pending.request, &request, self.config.coalesce_char_gap) {
                    pending.request = merge_requests(
                        &pending.request,
                        request,
                        self.config.window_size,
                        self.config.coalesce_char_gap,
                    );
                    pending.last_update = now;
                    (pending.generation, None)
                } else {
                    let flushed = pending.request.clone();
                    let generation = state.next_generation;
                    state.next_generation += 1;
                    state.pending = Some(PendingBatch {
                        request,
                        generation,
                        last_update: now,
                    });
                    (generation, Some(flushed))
                }
            } else {
                let generation = state.next_generation;
                state.next_generation += 1;
                state.pending = Some(PendingBatch {
                    request,
                    generation,
                    last_update: now,
                });
                (generation, None)
            }
        };

        if let Some(req) = flush_now {
            let sender = self.sender.clone();
            tokio::spawn(async move {
                let _ = sender.try_send(req);
            });
        }

        self.spawn_delayed_flush(batch_gen);
        Ok(())
    }

    fn spawn_delayed_flush(&self, generation: u64) {
        let sender = self.sender.clone();
        let state = self.state.clone();
        let batch_window = Duration::from_millis(self.config.batch_window_ms);

        tokio::spawn(async move {
            if !batch_window.is_zero() {
                tokio::time::sleep(batch_window).await;
            }

            let maybe_request = {
                let mut guard = state.lock().await;
                let should_flush = guard
                    .pending
                    .as_ref()
                    .map(|pending| {
                        pending.generation == generation
                            && (batch_window.is_zero()
                                || pending.last_update.elapsed() >= batch_window)
                    })
                    .unwrap_or(false);

                if should_flush {
                    guard.pending.take().map(|pending| pending.request)
                } else {
                    None
                }
            };

            if let Some(request) = maybe_request {
                let _ = sender.try_send(request);
            }
        });
    }
}

fn can_coalesce(existing: &RerankRequest, incoming: &RerankRequest, char_gap: usize) -> bool {
    if !same_buffer_neighborhood(&existing.context_left, &incoming.context_left, char_gap) {
        return false;
    }

    let existing_range = token_span(&existing.tokens);
    let incoming_range = token_span(&incoming.tokens);

    match (existing_range, incoming_range) {
        (Some((e_start, e_end)), Some((i_start, i_end))) => {
            e_start <= i_end.saturating_add(char_gap) && i_start <= e_end.saturating_add(char_gap)
        }
        _ => false,
    }
}

fn same_buffer_neighborhood(left: &str, right: &str, char_gap: usize) -> bool {
    if left == right {
        return true;
    }

    let left_chars = left.chars().count();
    let right_chars = right.chars().count();
    let diff = left_chars.abs_diff(right_chars);

    diff <= char_gap && (left.starts_with(right) || right.starts_with(left))
}

fn token_span(tokens: &[Token]) -> Option<(usize, usize)> {
    let start = tokens.iter().map(|t| t.start).min()?;
    let end = tokens.iter().map(|t| t.end).max()?;
    Some((start, end))
}

fn merge_requests(
    existing: &RerankRequest,
    incoming: RerankRequest,
    max_tokens: usize,
    char_gap: usize,
) -> RerankRequest {
    let mut merged = existing.tokens.clone();

    for token in incoming.tokens {
        if !merged.iter().any(|t| t == &token) {
            merged.push(token);
        }
    }

    merged.sort_by_key(|t| (t.start, t.end));

    if merged.len() > max_tokens {
        let mut best_slice_start = 0usize;
        let mut best_span = usize::MAX;
        for i in 0..=merged.len() - max_tokens {
            let span = merged[i + max_tokens - 1]
                .end
                .saturating_sub(merged[i].start);
            if span < best_span {
                best_span = span;
                best_slice_start = i;
            }
        }
        merged = merged[best_slice_start..best_slice_start + max_tokens].to_vec();
    }

    let context_left =
        if same_buffer_neighborhood(&existing.context_left, &incoming.context_left, char_gap)
            && incoming.context_left.chars().count() >= existing.context_left.chars().count()
        {
            incoming.context_left
        } else {
            existing.context_left.clone()
        };

    RerankRequest {
        tokens: merged,
        context_left,
        window_size: existing.window_size,
        threshold: existing.threshold,
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

    fn token_with_range(surface: &str, confidence: f32, start: usize, end: usize) -> Token {
        Token {
            surface: surface.to_string(),
            reading: surface.to_string(),
            pos: "noun".to_string(),
            pos_detail: "general".to_string(),
            start,
            end,
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
            batch_window_ms: 0,
            coalesce_char_gap: 6,
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
            batch_window_ms: 0,
            coalesce_char_gap: 6,
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
            batch_window_ms: 0,
            coalesce_char_gap: 6,
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

    #[tokio::test]
    async fn rerank_queue_coalesces_nearby_requests_in_batch_window() {
        let (queue, mut rx) = RerankQueue::new(RerankConfig {
            window_size: 5,
            threshold: 0.7,
            channel_capacity: 4,
            batch_window_ms: 50,
            coalesce_char_gap: 6,
        });

        let first = event_with_tokens(vec![token_with_range("A", 0.4, 0, 1)]);
        let second = event_with_tokens(vec![token_with_range("B", 0.3, 2, 3)]);

        queue.enqueue(first, "東京").await.unwrap();
        sleep(Duration::from_millis(15)).await;
        queue.enqueue(second, "東京都").await.unwrap();

        let req = timeout(Duration::from_millis(200), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");

        let surfaces: Vec<_> = req.tokens.iter().map(|t| t.surface.as_str()).collect();
        assert_eq!(surfaces, vec!["A", "B"]);
        assert_eq!(req.context_left, "東京都");

        let no_more = timeout(Duration::from_millis(80), rx.recv()).await;
        assert!(
            no_more.is_err(),
            "coalesced events should emit one batch only"
        );
    }

    #[tokio::test]
    async fn rerank_queue_splits_batch_when_buffer_moves_far() {
        let (queue, mut rx) = RerankQueue::new(RerankConfig {
            window_size: 5,
            threshold: 0.7,
            channel_capacity: 4,
            batch_window_ms: 80,
            coalesce_char_gap: 2,
        });

        let first = event_with_tokens(vec![token_with_range("A", 0.4, 0, 1)]);
        let second = event_with_tokens(vec![token_with_range("Z", 0.3, 40, 41)]);

        queue.enqueue(first, "abc").await.unwrap();
        sleep(Duration::from_millis(10)).await;
        queue.enqueue(second, "xyzxyzxyz").await.unwrap();

        let req1 = timeout(Duration::from_millis(150), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        let req2 = timeout(Duration::from_millis(220), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");

        assert_eq!(req1.tokens[0].surface, "A");
        assert_eq!(req2.tokens[0].surface, "Z");
    }

    #[tokio::test]
    async fn rerank_queue_respects_window_size_after_coalesce() {
        let (queue, mut rx) = RerankQueue::new(RerankConfig {
            window_size: 3,
            threshold: 0.7,
            channel_capacity: 4,
            batch_window_ms: 40,
            coalesce_char_gap: 8,
        });

        let first = event_with_tokens(vec![
            token_with_range("A", 0.2, 0, 1),
            token_with_range("B", 0.2, 1, 2),
            token_with_range("C", 0.2, 2, 3),
        ]);
        let second = event_with_tokens(vec![
            token_with_range("D", 0.2, 3, 4),
            token_with_range("E", 0.2, 4, 5),
        ]);

        queue.enqueue(first, "abc").await.unwrap();
        sleep(Duration::from_millis(10)).await;
        queue.enqueue(second, "abcde").await.unwrap();

        let req = timeout(Duration::from_millis(200), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");

        let surfaces: Vec<_> = req.tokens.iter().map(|t| t.surface.as_str()).collect();
        assert_eq!(surfaces, vec!["A", "B", "C"]);
    }
}
