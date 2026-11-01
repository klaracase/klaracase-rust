//! `request_duration` covers the final attempt only, and the `tracing` feature reports retries.

mod common;

use std::sync::Arc;
use std::time::Duration;

use klaracase::Klaracase;

use common::{FakeTime, MockResponse, MockServer};

const NORM_BODY: &str = include_str!("fixtures/examples/norms.show.json");
const UNAVAILABLE: &str = r#"{"type":"about:blank","title":"Service Unavailable","status":503,"detail":"later","code":"service_unavailable","retry_after_seconds":1}"#;

fn client_for(server: &MockServer, time: &FakeTime) -> Klaracase {
    Klaracase::builder()
        .token("klc_your_token_here")
        .base_url(server.base_url())
        .sleeper(Arc::new(time.clone()))
        .clock(Arc::new(time.clone()))
        .build()
        .expect("client")
}

#[tokio::test]
async fn request_duration_counts_the_final_attempt_only() {
    let slow_failure = MockResponse {
        delay: Duration::from_millis(600),
        ..MockResponse::json(503, UNAVAILABLE)
    };
    let server = MockServer::start(
        vec![slow_failure, MockResponse::json(200, NORM_BODY)],
        false,
    )
    .await;
    let time = FakeTime::new();
    let client = client_for(&server, &time);

    let norm = client
        .norms()
        .get("§ 535 BGB")
        .await
        .expect("second attempt");

    assert_eq!(server.requests().len(), 2, "one retry happened");
    assert!(
        norm.meta.request_duration < Duration::from_millis(400),
        "the 600 ms of the first attempt must not count: {:?}",
        norm.meta.request_duration
    );
}

#[cfg(feature = "tracing")]
mod events {
    use std::fmt::Write as _;
    use std::sync::{Arc, Mutex};

    use tracing::field::{Field, Visit};
    use tracing::span::{Attributes, Id, Record};
    use tracing::{Event, Level, Metadata, Subscriber};

    use super::*;

    #[derive(Default)]
    struct Line(String);

    impl Visit for Line {
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            let _ = write!(self.0, "{}={value:?} ", field.name());
        }
    }

    struct Capture(Arc<Mutex<Vec<(Level, String)>>>);

    impl Subscriber for Capture {
        fn enabled(&self, _: &Metadata<'_>) -> bool {
            true
        }
        fn new_span(&self, _: &Attributes<'_>) -> Id {
            Id::from_u64(1)
        }
        fn record(&self, _: &Id, _: &Record<'_>) {}
        fn record_follows_from(&self, _: &Id, _: &Id) {}
        fn event(&self, event: &Event<'_>) {
            let mut line = Line::default();
            event.record(&mut line);
            self.0
                .lock()
                .unwrap()
                .push((*event.metadata().level(), line.0));
        }
        fn enter(&self, _: &Id) {}
        fn exit(&self, _: &Id) {}
    }

    #[tokio::test]
    async fn a_retry_is_a_debug_event_without_the_token() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let _guard = tracing::subscriber::set_default(Capture(Arc::clone(&events)));

        let server = MockServer::start(
            vec![
                MockResponse::json(503, UNAVAILABLE),
                MockResponse::json(200, NORM_BODY),
            ],
            false,
        )
        .await;
        let time = FakeTime::new();
        client_for(&server, &time)
            .norms()
            .get("§ 535 BGB")
            .await
            .expect("second attempt");

        let events = events.lock().unwrap();
        let retries: Vec<_> = events
            .iter()
            .filter(|(_, line)| line.contains("retrying"))
            .collect();
        assert_eq!(retries.len(), 1, "{events:?}");
        let (level, line) = retries[0];
        assert_eq!(*level, Level::DEBUG);
        for needle in [
            "method=GET",
            "status=503",
            "attempt=1",
            "wait_ms=1000",
            "/norms/",
        ] {
            assert!(line.contains(needle), "`{line}` lacks `{needle}`");
        }
        for (_, line) in events.iter() {
            assert!(!line.contains("klc_"), "the token leaked: {line}");
        }
    }

    #[tokio::test]
    async fn a_transport_retry_has_no_status() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let _guard = tracing::subscriber::set_default(Capture(Arc::clone(&events)));

        let server = MockServer::start(
            vec![MockResponse::dropped(), MockResponse::json(200, NORM_BODY)],
            false,
        )
        .await;
        let time = FakeTime::new();
        client_for(&server, &time)
            .norms()
            .get("§ 535 BGB")
            .await
            .expect("second attempt");

        let events = events.lock().unwrap();
        let (_, line) = events
            .iter()
            .find(|(_, line)| line.contains("retrying"))
            .expect("a retry event");
        assert!(!line.contains("status=Some"), "{line}");
        assert!(line.contains("attempt=1"), "{line}");
    }

    #[tokio::test]
    async fn a_deprecation_is_one_warning() {
        klaracase::reset_deprecation_guard();
        let events = Arc::new(Mutex::new(Vec::new()));
        let _guard = tracing::subscriber::set_default(Capture(Arc::clone(&events)));

        let mut response = MockResponse::json(200, NORM_BODY);
        response.headers.extend([
            ("deprecation".to_owned(), "true".to_owned()),
            (
                "sunset".to_owned(),
                "Wed, 01 Jul 2027 00:00:00 GMT".to_owned(),
            ),
        ]);
        let server = MockServer::start(vec![response], true).await;
        let client = client_for(&server, &FakeTime::new());
        client.norms().get("§ 535 BGB").await.expect("first");
        client.norms().get("§ 535 BGB").await.expect("second");

        let events = events.lock().unwrap();
        let warnings: Vec<_> = events
            .iter()
            .filter(|(level, _)| *level == Level::WARN)
            .collect();
        assert_eq!(warnings.len(), 1, "{events:?}");
        assert!(warnings[0].1.contains("Sunset: Wed, 01 Jul 2027"));
        assert!(!warnings[0].1.contains("klc_"));
    }
}
