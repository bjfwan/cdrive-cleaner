use std::time::Instant;

pub struct StageTimer {
    scope: String,
    step: String,
    start: Instant,
    finished: bool,
}

impl StageTimer {
    pub fn start(scope: impl Into<String>, step: impl Into<String>) -> Self {
        Self {
            scope: scope.into(),
            step: step.into(),
            start: Instant::now(),
            finished: false,
        }
    }

    pub fn finish_with(mut self, detail: impl AsRef<str>) {
        self.finished = true;
        log_elapsed(&self.scope, &self.step, self.start, detail.as_ref());
    }
}

impl Drop for StageTimer {
    fn drop(&mut self) {
        if !self.finished {
            println!(
                "[scan-timing][{}] {} aborted_after={:.2}ms",
                self.scope,
                self.step,
                elapsed_ms(self.start)
            );
        }
    }
}

pub fn elapsed_ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

pub fn log_elapsed(scope: &str, step: &str, start: Instant, detail: impl AsRef<str>) {
    let detail = detail.as_ref();
    if detail.is_empty() {
        println!(
            "[scan-timing][{}] {} took {:.2}ms",
            scope,
            step,
            elapsed_ms(start)
        );
    } else {
        println!(
            "[scan-timing][{}] {} took {:.2}ms | {}",
            scope,
            step,
            elapsed_ms(start),
            detail
        );
    }
}
