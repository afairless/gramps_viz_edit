//! Progress reporting for large generation runs.
//!
//! The reporter prints progress updates to stderr at configurable intervals.

/// Reports generation progress to stderr.
///
/// The reporter prints a progress line every `interval` persons
/// during generation, showing the current count and total.
#[derive(Clone, Debug)]
pub struct ProgressReporter {
    /// How often to report (every N persons).
    interval: usize,
    /// Total number of persons to generate.
    total: usize,
    /// Current count of generated persons.
    current: usize,
}

impl ProgressReporter {
    /// Create a new `ProgressReporter`.
    ///
    /// When `interval` is 0, progress reporting is disabled (no output).
    pub fn new(interval: usize, total: usize) -> Self {
        ProgressReporter {
            interval,
            total,
            current: 0,
        }
    }

    /// Advance the counter by one and print progress if at interval.
    pub fn tick(&mut self) {
        self.current += 1;
        if self.interval > 0 && self.current.is_multiple_of(self.interval) {
            eprintln!(
                "Progress: {}/{} persons generated...",
                self.current, self.total
            );
        }
    }

    /// Print the final progress line.
    pub fn finish(&self) {
        if self.interval > 0 {
            eprintln!(
                "Progress: {}/{} persons generated... done.",
                self.current, self.total
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_reporter_new() {
        let reporter = ProgressReporter::new(100, 500);
        assert_eq!(reporter.interval, 100);
        assert_eq!(reporter.total, 500);
        assert_eq!(reporter.current, 0);
    }

    #[test]
    fn progress_reporter_tick_silent_below_interval() {
        let mut reporter = ProgressReporter::new(100, 500);
        for _ in 0..99 {
            reporter.tick();
        }
        // No output expected — just checking it doesn't panic
        assert_eq!(reporter.current, 99);
    }

    #[test]
    fn progress_reporter_tick_output_at_interval() {
        let mut reporter = ProgressReporter::new(100, 500);
        for _ in 0..99 {
            reporter.tick();
        }
        assert_eq!(reporter.current, 99);
        // 100th tick should trigger output
        reporter.tick();
        assert_eq!(reporter.current, 100);
    }

    #[test]
    fn progress_reporter_finish_output() {
        let reporter = ProgressReporter::new(100, 500);
        // finish() just prints, shouldn't panic
        reporter.finish();
    }

    #[test]
    fn progress_reporter_zero_interval() {
        let mut reporter = ProgressReporter::new(0, 500);
        for _ in 0..1000 {
            reporter.tick();
        }
        assert_eq!(reporter.current, 1000);
        reporter.finish();
    }

    #[test]
    fn progress_reporter_tick_exact_boundary() {
        let mut reporter = ProgressReporter::new(50, 200);
        for _ in 0..50 {
            reporter.tick();
        }
        assert_eq!(reporter.current, 50);
    }

    #[test]
    fn progress_reporter_tick_above_interval() {
        let mut reporter = ProgressReporter::new(50, 200);
        for _ in 0..53 {
            reporter.tick();
        }
        assert_eq!(reporter.current, 53);
    }
}
