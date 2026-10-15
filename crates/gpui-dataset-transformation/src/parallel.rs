//! Index-parallel map for heavy filters.
//!
//! On native targets the index range is split across scoped threads;
//! results are collected in index order, so parallel output is bit-for-
//! bit identical to the sequential result. On wasm the same call runs
//! sequentially (no thread spawning without shared-memory atomics).

// Rust guideline compliant 2026-02-21

/// Maps `f` over `0..len`, preserving order in the output.
///
/// # Panics
///
/// Panics when a worker thread panics.
pub(crate) fn parallel_map<T, F>(len: usize, f: F) -> Vec<T>
where
    T: Send,
    F: Fn(usize) -> T + Sync,
{
    #[cfg(target_family = "wasm")]
    {
        return (0..len).map(f).collect();
    }
    #[cfg(not(target_family = "wasm"))]
    {
        let threads = std::thread::available_parallelism()
            .map_or(1, std::num::NonZero::get)
            .min(len.max(1));
        if threads <= 1 || len < 4096 {
            return (0..len).map(f).collect();
        }
        let chunk = len.div_ceil(threads);
        let mut out: Vec<T> = Vec::with_capacity(len);
        std::thread::scope(|scope| {
            let mut handles = Vec::new();
            let mut start = 0;
            while start < len {
                let end = (start + chunk).min(len);
                let f_ref = &f;
                handles.push(scope.spawn(move || (start..end).map(f_ref).collect::<Vec<T>>()));
                start = end;
            }
            for handle in handles {
                out.extend(handle.join().expect("parallel worker panicked"));
            }
        });
        out
    }
}

/// Maps `f` over contiguous index ranges (one call per worker).
///
/// # Panics
///
/// Panics when a worker thread panics.
pub(crate) fn parallel_ranges<T, F>(len: usize, f: F) -> Vec<T>
where
    T: Send,
    F: Fn(std::ops::Range<usize>) -> T + Sync,
{
    #[cfg(target_family = "wasm")]
    {
        return vec![f(0..len)];
    }
    #[cfg(not(target_family = "wasm"))]
    {
        let threads = std::thread::available_parallelism()
            .map_or(1, std::num::NonZero::get)
            .min(len.max(1));
        if threads <= 1 || len < 4096 {
            return vec![f(0..len)];
        }
        let chunk = len.div_ceil(threads);
        let mut out = Vec::new();
        std::thread::scope(|scope| {
            let mut handles = Vec::new();
            let mut start = 0;
            while start < len {
                let end = (start + chunk).min(len);
                let f_ref = &f;
                handles.push(scope.spawn(move || f_ref(start..end)));
                start = end;
            }
            for handle in handles {
                out.push(handle.join().expect("parallel worker panicked"));
            }
        });
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parallel_map_preserves_order() {
        let values = parallel_map(10_000, |i| i * 2);
        assert_eq!(values.len(), 10_000);
        assert!(values.iter().enumerate().all(|(i, v)| *v == i * 2));
    }
}
