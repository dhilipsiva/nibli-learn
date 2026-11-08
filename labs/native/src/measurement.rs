//! Measurement is a local observation, never a performance ranking.
use nibli_session::CoreSession;
use serde_json::{Value, json};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;
struct Counting;
static ALLOCS: AtomicU64 = AtomicU64::new(0);
static BYTES: AtomicU64 = AtomicU64::new(0);
static REALLOCS: AtomicU64 = AtomicU64::new(0);
#[global_allocator]
static ALLOCATOR: Counting = Counting;
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc(layout) };
        if !p.is_null() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        p
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let p = unsafe { System.alloc_zeroed(layout) };
        if !p.is_null() {
            ALLOCS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        p
    }
    unsafe fn dealloc(&self, p: *mut u8, layout: Layout) {
        unsafe {
            System.dealloc(p, layout);
        }
    }
    unsafe fn realloc(&self, p: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        let new = unsafe { System.realloc(p, layout, size) };
        if !new.is_null() {
            REALLOCS.fetch_add(1, Ordering::Relaxed);
            BYTES.fetch_add(size as u64, Ordering::Relaxed);
        }
        new
    }
}
fn counts() -> (u64, u64, u64) {
    (
        ALLOCS.load(Ordering::Relaxed),
        BYTES.load(Ordering::Relaxed),
        REALLOCS.load(Ordering::Relaxed),
    )
}
fn rss_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find(|l| l.starts_with("VmRSS:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}
pub(crate) fn phase<T>(name: &str, work: impl FnOnce() -> T) -> (T, Value) {
    let rss_before = rss_kib();
    let before = counts();
    let began = Instant::now();
    let value = work();
    let elapsed_ns = began.elapsed().as_nanos();
    let after = counts();
    let rss_after = rss_kib();
    (
        value,
        json!({"phase":name,"elapsed_ns":elapsed_ns,"allocation_calls":after.0-before.0,"requested_allocation_bytes":after.1-before.1,"reallocation_calls":after.2-before.2,"rss_before_kib":rss_before,"rss_after_kib":rss_after}),
    )
}
fn workload(name: &str) -> (Vec<String>, String) {
    let mut rows = vec!["approves(Stage0, CartB).".into()];
    match name {
        "deep-narrow" => {
            for i in 0..12 {
                rows.push(format!(
                    "all $cart: approves(Stage{i}, $cart) -> approves(Stage{}, $cart).",
                    i + 1
                ));
            }
        }
        "wide" => {
            rows.clear();
            for i in 0..32 {
                rows.push(format!("approves(Reviewer{i}, CartB)."));
                rows.push(format!(
                    "all $cart: approves(Reviewer{i}, $cart) -> approves(Stage12, $cart)."
                ));
            }
        }
        "sparse" => {
            rows.push("all $cart: approves(Stage0, $cart) -> approves(Stage12, $cart).".into());
            for i in 0..512 {
                rows.push(format!("computer(Terminal{i})."));
            }
        }
        _ => unreachable!(),
    }
    (rows, "approves(Stage12, CartB).".into())
}
pub fn run() -> Value {
    let mut observations = vec![];
    for name in ["deep-narrow", "wide", "sparse"] {
        let (inputs, query) = workload(name);
        for repetition in 0..3 {
            let (core, init) = phase("session-initialization", CoreSession::new);
            core.set_max_chain_depth(32).unwrap();
            let (compiled, compile) = phase("compile-inputs-only", || {
                inputs
                    .iter()
                    .map(|s| core.compile_text(s).unwrap())
                    .collect::<Vec<_>>()
            });
            // assert_text deliberately includes compilation; report that boundary honestly.
            let (_, admission) = phase("text-compilation-and-admission", || {
                for text in &inputs {
                    core.assert_text(text).unwrap();
                }
            });
            let (decision, holds) = phase("text-query-without-proof", || {
                core.query_text(&query).unwrap()
            });
            assert!(decision.is_true());
            let ((proved, proof), proving) = phase("text-query-and-proof", || {
                core.query_text_with_proof(&query).unwrap()
            });
            assert_eq!(decision, proved);
            let (serialized, serialization) = phase("proof-json-serialization", || {
                serde_json::to_vec(&proof).unwrap()
            });
            observations.push(json!({"workload":name,"repetition":repetition,"profile":{"max_chain_depth":32},"inputs":inputs,"query":query,"compiled_node_count":compiled.iter().map(|b|b.nodes.len()).sum::<usize>(),"assertion_count":core.list_assertion_records().unwrap().len(),"decision":decision,"proof_bytes":serialized.len(),"phases":[init,compile,admission,holds,proving,serialization]}));
        }
    }
    json!({"runs":observations,"method":{"clock":"std::time::Instant monotonic elapsed time","allocator":"System wrapper counts successful allocation/reallocation requests in this single process; overhead is included","bytes":"requested bytes, counting reallocations at their new size; not net live memory or peak usage","rss":"Linux /proc/self/status VmRSS; null means unavailable; includes process and retained allocator pages","repetition":"three runs per shape in one warmed process; not isolated trials or a benchmark comparison","profile":"native release build; no WASI host fuel/memory cap; proof and non-proof query measured separately"},"checks":{"all_queries_true":true,"distinct_shapes":observations.len()==9}})
}
