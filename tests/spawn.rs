//! The harness's own guarantee, kept in one binary so `cargo nextest run
//! spawn_is_scrubbed` finds exactly one test named that.

mod common;

#[test]
fn spawn_is_scrubbed() {
    common::assert_spawn_is_scrubbed();
}
