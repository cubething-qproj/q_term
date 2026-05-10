//! Direct unit tests for [`PendingTermInput::push_writes`] eviction
//! behaviour. These don't touch a Bevy `App`; the method takes the
//! cap as a `usize` parameter so we can pass small values and observe
//! FIFO whole-span eviction.

use crate::prelude::*;

/// Convenience: build a [`TermWrite`] with the given literal text.
fn span(text: &str) -> TermWrite {
    TermWrite::new(text)
}

/// Sum of `text` lengths across the pending queue.
fn total_bytes(p: &PendingTermInput) -> usize {
    p.writes.iter().map(|w| w.text.len()).sum()
}

/// Pushing spans whose total size stays under the cap evicts nothing
/// and preserves insertion order.
#[test]
fn under_cap_no_eviction() {
    let mut p = PendingTermInput::default();
    let evicted = p.push_writes([span("aa"), span("bb"), span("cc")], 32);
    assert_eq!(evicted, 0);
    let texts: Vec<_> = p.writes.iter().map(|w| w.text.as_str()).collect();
    assert_eq!(texts, ["aa", "bb", "cc"]);
    assert!(total_bytes(&p) <= 32);
}

/// A push that pulls the queue over the cap evicts oldest spans FIFO
/// until the queue fits again.
#[test]
fn single_push_over_cap_evicts_fifo() {
    let mut p = PendingTermInput::default();
    // Pre-fill near the cap.
    let pre = p.push_writes(
        [span("aaaa"), span("bbbb"), span("cccc"), span("dddd")],
        16,
    );
    assert_eq!(pre, 0);
    assert_eq!(total_bytes(&p), 16);

    // One more 4-byte span: total would be 20 > 16. Oldest must go.
    let evicted = p.push_writes([span("eeee")], 16);
    assert!(evicted > 0, "expected eviction, got {evicted}");
    let texts: Vec<_> = p.writes.iter().map(|w| w.text.as_str()).collect();
    assert_eq!(texts, ["bbbb", "cccc", "dddd", "eeee"]);
    assert!(total_bytes(&p) <= 16);
}

/// A single span larger than the cap is still admitted (dropping it
/// would silently lose mid-stream data); every prior span is evicted.
#[test]
fn oversize_single_span_admitted() {
    let mut p = PendingTermInput::default();
    p.push_writes([span("aa"), span("bb"), span("cc")], 16);
    let prior_len = p.writes.len();

    let big = "x".repeat(64);
    let evicted = p.push_writes([span(&big)], 16);

    // All prior spans evicted; the oversized span remains.
    assert_eq!(evicted, prior_len);
    assert_eq!(p.writes.len(), 1);
    assert_eq!(p.writes[0].text.len(), 64);
}

/// Pushing several new spans in one call applies eviction across the
/// whole batch; the survivors are the most recent FIFO suffix.
#[test]
fn batched_push_mixed_eviction() {
    let mut p = PendingTermInput::default();
    p.push_writes([span("aa"), span("bb")], 16);
    assert_eq!(total_bytes(&p), 4);

    // Push four 4-byte spans: pre-existing 4 + new 16 = 20 > 16, so
    // oldest spans get evicted as the batch lands.
    let evicted = p.push_writes(
        [span("cccc"), span("dddd"), span("eeee"), span("ffff")],
        16,
    );
    assert!(evicted >= 1);
    let texts: Vec<_> = p.writes.iter().map(|w| w.text.as_str()).collect();
    assert_eq!(texts, ["cccc", "dddd", "eeee", "ffff"]);
    assert!(total_bytes(&p) <= 16);
}
