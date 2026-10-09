//! `tracing_init::install_panic_hook` (issue #121): Rust panics become
//! structured `forge.panic` events, and never vanish when nothing records
//! that target.
//!
//! The panic hook is process-global, so this file holds a single test that
//! runs its scenarios in order.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use forge_lang::runtime::tracing_init;
use tracing::field::{Field, Visit};
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;

#[derive(Debug, Default, Clone)]
struct Recorded {
    target: String,
    level: String,
    fields: Vec<(String, String)>,
    spans: Vec<String>,
}

#[derive(Clone, Default)]
struct Recorder(Arc<Mutex<Vec<Recorded>>>);

struct FieldVisitor<'a>(&'a mut Vec<(String, String)>);

impl Visit for FieldVisitor<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0
            .push((field.name().to_string(), format!("{value:?}")));
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.push((field.name().to_string(), value.to_string()));
    }
}

impl<S> Layer<S> for Recorder
where
    S: Subscriber + for<'a> tracing_subscriber::registry::LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let mut rec = Recorded {
            target: event.metadata().target().to_string(),
            level: event.metadata().level().to_string(),
            ..Default::default()
        };
        event.record(&mut FieldVisitor(&mut rec.fields));
        if let Some(scope) = ctx.event_scope(event) {
            rec.spans = scope.from_root().map(|s| s.name().to_string()).collect();
        }
        self.0.lock().unwrap_or_else(|p| p.into_inner()).push(rec);
    }
}

fn field<'a>(rec: &'a Recorded, name: &str) -> Option<&'a str> {
    rec.fields
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}

#[test]
fn panics_are_reported_as_tracing_events_with_fallback() {
    // A stand-in for the default hook, so we can see when it runs.
    let previous_calls = Arc::new(AtomicUsize::new(0));
    {
        let previous_calls = Arc::clone(&previous_calls);
        std::panic::set_hook(Box::new(move |_| {
            previous_calls.fetch_add(1, Ordering::SeqCst);
        }));
    }
    tracing_init::install_panic_hook();

    // 1. A subscriber that records forge.panic: one structured event, inside
    //    the current span, and the previous hook stays quiet.
    let recorder = Recorder::default();
    let subscriber = tracing_subscriber::registry().with(
        recorder.clone().with_filter(
            tracing_subscriber::filter::Targets::new()
                .with_target(tracing_init::PANIC_TARGET, tracing::Level::ERROR)
                .with_target("panic_hook", tracing::Level::INFO),
        ),
    );
    tracing::subscriber::with_default(subscriber, || {
        let span = tracing::info_span!("request", method = "GET");
        let _entered = span.enter();
        let result = std::panic::catch_unwind(|| panic!("boom {}", 7));
        assert!(result.is_err());
    });
    let events = recorder.0.lock().unwrap_or_else(|p| p.into_inner()).clone();
    assert_eq!(events.len(), 1, "exactly one panic event: {events:?}");
    let event = &events[0];
    assert_eq!(event.target, tracing_init::PANIC_TARGET);
    assert_eq!(event.level, "ERROR");
    assert_eq!(field(event, "payload"), Some("boom 7"));
    let location = field(event, "location").expect("location");
    assert!(location.contains("panic_hook.rs"), "{location}");
    assert!(field(event, "thread").is_some());
    assert_eq!(event.spans, vec!["request".to_string()]);
    assert_eq!(previous_calls.load(Ordering::SeqCst), 0);

    // 2. A subscriber whose filter excludes forge.panic: the previous hook
    //    reports the panic instead of it being dropped.
    let quiet = Recorder::default();
    let subscriber = tracing_subscriber::registry().with(quiet.clone().with_filter(
        tracing_subscriber::filter::Targets::new().with_target("forge.user", tracing::Level::INFO),
    ));
    tracing::subscriber::with_default(subscriber, || {
        let result = std::panic::catch_unwind(|| panic!("unrecorded"));
        assert!(result.is_err());
    });
    assert!(quiet.0.lock().unwrap_or_else(|p| p.into_inner()).is_empty());
    assert_eq!(previous_calls.load(Ordering::SeqCst), 1);

    // 3. No subscriber at all: also the previous hook.
    let result = std::panic::catch_unwind(|| panic!("no subscriber"));
    assert!(result.is_err());
    assert_eq!(previous_calls.load(Ordering::SeqCst), 2);

    let _ = std::panic::take_hook();
}
