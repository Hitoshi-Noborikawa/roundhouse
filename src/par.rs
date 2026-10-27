//! Deterministic data parallelism for the compiler's independent per-item
//! work: results come back in input order whatever the thread timing, so
//! the output never depends on how many cores ran it.
//!
//! Host-only threads: wasm runs [`map`] and [`join`] on the calling thread.
//!
//! Most of the compiler's ambient state is thread-local (the ingest's source
//! registry, diagnostics, the plan caches, the app's inflections), and a
//! worker starts with none of it, so work handed to a worker must not read
//! or write any: pass what it needs in through the closure.

/// `f` over `items`, one result per item in input order. Workers take
/// items from a shared counter on threads with the compiler's stack
/// budget (the closure may walk expression trees).
pub fn map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let workers = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get).min(32).min(items.len());
        if workers > 1 {
            let next = AtomicUsize::new(0);
            let (next, f) = (&next, &f);
            let done: Vec<Vec<(usize, R)>> = std::thread::scope(|scope| {
                let handles: Vec<_> = (0..workers)
                    .map(|_| {
                        std::thread::Builder::new()
                            .name("roundhouse-par".into())
                            .stack_size(crate::stack::COMPILER_STACK_BYTES)
                            .spawn_scoped(scope, move || {
                                let mut mine = Vec::new();
                                loop {
                                    let index = next.fetch_add(1, Ordering::Relaxed);
                                    let Some(item) = items.get(index) else { break };
                                    mine.push((index, f(item)));
                                }
                                mine
                            })
                            .expect("spawn a parallel worker")
                    })
                    .collect();
                handles.into_iter().map(|h| h.join().unwrap_or_else(|payload| std::panic::resume_unwind(payload))).collect()
            });
            let mut slots: Vec<Option<R>> = items.iter().map(|_| None).collect();
            for (index, result) in done.into_iter().flatten() {
                slots[index] = Some(result);
            }
            return slots.into_iter().map(|r| r.expect("every item was mapped")).collect();
        }
    }
    items.iter().map(f).collect()
}

/// `a` and `b`, concurrently where there are threads; both results, as
/// if run one after the other.
pub fn join<A: Send, B: Send>(a: impl FnOnce() -> A + Send, b: impl FnOnce() -> B + Send) -> (A, B) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::thread::scope(|scope| {
            let other = std::thread::Builder::new()
                .name("roundhouse-par".into())
                .stack_size(crate::stack::COMPILER_STACK_BYTES)
                .spawn_scoped(scope, b)
                .expect("spawn a parallel worker");
            let first = a();
            let second = other.join().unwrap_or_else(|payload| std::panic::resume_unwind(payload));
            (first, second)
        })
    }
    #[cfg(target_arch = "wasm32")]
    {
        (a(), b())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_returns_results_in_input_order() {
        let items: Vec<usize> = (0..1000).collect();
        // Early items take longest, so a worker-order result would be reversed.
        let out = map(&items, |&n| {
            if n < 8 {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            n * 2
        });
        assert_eq!(out, items.iter().map(|n| n * 2).collect::<Vec<_>>());
        assert!(map::<usize, usize>(&[], |&n| n).is_empty());
        assert_eq!(map(&[7], |&n| n + 1), vec![8]);
    }

    #[test]
    fn join_returns_both_results_in_argument_order() {
        assert_eq!(join(|| "a".to_string(), || 2), ("a".to_string(), 2));
    }

    #[test]
    #[should_panic(expected = "worker failed")]
    fn a_panicking_worker_propagates() {
        map(&[1, 2, 3, 4], |&n| if n == 3 { panic!("worker failed") } else { n });
    }
}
