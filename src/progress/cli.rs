use super::*;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle as IndicatifStyle};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// CLI implementation of ProgressReporter using indicatif
///
/// Wraps MultiProgress and maintains a mapping from ProgressId to ProgressBar.
/// This preserves all existing CLI terminal output behavior.
pub struct CliProgressReporter {
    multi: Arc<MultiProgress>,
    bars: Arc<Mutex<HashMap<ProgressId, ProgressBar>>>,
}

impl CliProgressReporter {
    /// Create a new CLI progress reporter wrapping a MultiProgress instance
    pub fn new(multi: MultiProgress) -> Self {
        Self {
            multi: Arc::new(multi),
            bars: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

impl ProgressReporter for CliProgressReporter {
    fn section_start(&self, stage_index: u32, message: &str) -> ProgressId {
        let id = ProgressId::new();
        let bar = self.multi.add(
            ProgressBar::new_spinner()
                .with_style(
                    IndicatifStyle::default_spinner()
                        .template("{prefix}{spinner} {msg}")
                        .unwrap(),
                )
                .with_prefix(format!("[{}/3]", stage_index))
                .with_message(message.to_string()),
        );
        bar.enable_steady_tick(Duration::from_millis(250));

        self.bars.lock().unwrap().insert(id, bar);
        id
    }

    fn section_finish(&self, id: ProgressId, message: &str) {
        if let Some(bar) = self.bars.lock().unwrap().get(&id) {
            bar.finish_with_message(message.to_string());
        }
    }

    fn create(&self, config: ProgressConfig) -> ProgressId {
        let id = ProgressId::new();
        let bar = match config.style {
            ProgressStyle::Spinner => {
                let bar = self.multi.add(
                    ProgressBar::new_spinner()
                        .with_style(
                            IndicatifStyle::default_spinner()
                                .template("{prefix}{spinner} {msg}")
                                .unwrap(),
                        )
                        .with_prefix(config.prefix)
                        .with_message(config.message),
                );
                if let Some(interval) = config.steady_tick {
                    bar.enable_steady_tick(interval);
                }
                bar
            }
            ProgressStyle::Bar { total } => self.multi.add(
                ProgressBar::new(total)
                    .with_style(
                        IndicatifStyle::default_bar()
                            .template("{prefix}{msg} [{bar:80}] {pos}/{len} (Remaining: {eta})")
                            .unwrap()
                            .progress_chars("#>-"),
                    )
                    .with_prefix(config.prefix)
                    .with_message(config.message),
            ),
        };

        self.bars.lock().unwrap().insert(id, bar);
        id
    }

    fn update_work(&self, id: ProgressId, work: WorkUpdate) {
        // Format semantic work update to display string for indicatif
        let message = match work {
            WorkUpdate::Directory { path } => format!("Directory: '{}'", path),
            WorkUpdate::File { path } => format!("Item: '{}'", path),
            WorkUpdate::Hashing { file } => format!("Hashing: '{}'", file),
            WorkUpdate::Validating { file } => format!("Validating: '{}'", file),
            WorkUpdate::Idle => "Waiting...".to_string(),
        };

        if let Some(bar) = self.bars.lock().unwrap().get(&id) {
            bar.set_message(message);
        }
    }

    fn set_position(&self, id: ProgressId, position: u64) {
        if let Some(bar) = self.bars.lock().unwrap().get(&id) {
            bar.set_position(position);
        }
    }

    fn set_length(&self, id: ProgressId, length: u64) {
        if let Some(bar) = self.bars.lock().unwrap().get(&id) {
            bar.set_length(length);
        }
    }

    fn inc(&self, id: ProgressId, delta: u64) {
        if let Some(bar) = self.bars.lock().unwrap().get(&id) {
            bar.inc(delta);
        }
    }

    fn enable_steady_tick(&self, id: ProgressId, interval: Duration) {
        if let Some(bar) = self.bars.lock().unwrap().get(&id) {
            bar.enable_steady_tick(interval);
        }
    }

    fn disable_steady_tick(&self, id: ProgressId) {
        if let Some(bar) = self.bars.lock().unwrap().get(&id) {
            bar.disable_steady_tick();
        }
    }

    fn finish_and_clear(&self, id: ProgressId) {
        if let Some(bar) = self.bars.lock().unwrap().remove(&id) {
            bar.finish_and_clear();
        }
    }

    fn println(&self, message: &str) -> Result<(), Box<dyn std::error::Error>> {
        self.multi.println(message)?;
        Ok(())
    }

    fn clone_reporter(&self) -> Arc<dyn ProgressReporter> {
        Arc::new(Self {
            multi: Arc::clone(&self.multi),
            bars: Arc::clone(&self.bars),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_reporter_creation() {
        let multi = MultiProgress::new();
        let reporter = CliProgressReporter::new(multi);

        // Verify internal structure
        assert_eq!(reporter.bars.lock().unwrap().len(), 0);
    }

    #[test]
    fn test_section_start_creates_progress_id() {
        let multi = MultiProgress::new();
        let reporter = CliProgressReporter::new(multi);

        let id = reporter.section_start(1, "Test section");

        // Verify a progress bar was created and stored
        assert_eq!(reporter.bars.lock().unwrap().len(), 1);
        assert!(reporter.bars.lock().unwrap().contains_key(&id));
    }

    #[test]
    fn test_create_spinner() {
        let multi = MultiProgress::new();
        let reporter = CliProgressReporter::new(multi);

        let config = ProgressConfig {
            style: ProgressStyle::Spinner,
            prefix: "   ".to_string(),
            message: "Loading...".to_string(),
            steady_tick: None,
        };

        let id = reporter.create(config);

        assert_eq!(reporter.bars.lock().unwrap().len(), 1);
        assert!(reporter.bars.lock().unwrap().contains_key(&id));
    }

    #[test]
    fn test_create_progress_bar() {
        let multi = MultiProgress::new();
        let reporter = CliProgressReporter::new(multi);

        let config = ProgressConfig {
            style: ProgressStyle::Bar { total: 100 },
            prefix: "[1/3]".to_string(),
            message: "Files".to_string(),
            steady_tick: None,
        };

        let id = reporter.create(config);

        assert_eq!(reporter.bars.lock().unwrap().len(), 1);
        assert!(reporter.bars.lock().unwrap().contains_key(&id));
    }

    #[test]
    fn test_multiple_progress_indicators() {
        let multi = MultiProgress::new();
        let reporter = CliProgressReporter::new(multi);

        let id1 = reporter.section_start(1, "Section 1");
        let id2 = reporter.create(ProgressConfig {
            style: ProgressStyle::Spinner,
            prefix: "".to_string(),
            message: "Spinner".to_string(),
            steady_tick: None,
        });
        let id3 = reporter.create(ProgressConfig {
            style: ProgressStyle::Bar { total: 50 },
            prefix: "".to_string(),
            message: "Bar".to_string(),
            steady_tick: None,
        });

        assert_eq!(reporter.bars.lock().unwrap().len(), 3);
        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
        assert_ne!(id1, id3);
    }

    #[test]
    fn test_finish_and_clear_removes_progress_bar() {
        let multi = MultiProgress::new();
        let reporter = CliProgressReporter::new(multi);

        let id = reporter.section_start(1, "Test");
        assert_eq!(reporter.bars.lock().unwrap().len(), 1);

        reporter.finish_and_clear(id);
        assert_eq!(reporter.bars.lock().unwrap().len(), 0);
    }

    #[test]
    fn test_update_work_on_nonexistent_id() {
        let multi = MultiProgress::new();
        let reporter = CliProgressReporter::new(multi);

        let fake_id = ProgressId::new();
        // Should not panic
        reporter.update_work(fake_id, WorkUpdate::Idle);
    }

    #[test]
    fn test_clone_reporter() {
        let multi = MultiProgress::new();
        let reporter = CliProgressReporter::new(multi);

        let id = reporter.section_start(1, "Test");

        let cloned = reporter.clone_reporter();

        // Cloned reporter should share the same state
        // We can't directly test Arc equality, but we can verify behavior
        cloned.update_work(id, WorkUpdate::Hashing {
            file: "test.txt".to_string(),
        });

        // Both should reference the same progress bar
        assert!(reporter.bars.lock().unwrap().contains_key(&id));
    }
}
