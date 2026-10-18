use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Labels,
    Layout,
    Syntax,
    Detail,
    Language,
    Projection,
    Parse,
    Query,
}

impl Stage {
    /// Every stage, in report order.
    pub const ALL: [Self; 8] = [
        Self::Labels,
        Self::Layout,
        Self::Syntax,
        Self::Detail,
        Self::Language,
        Self::Projection,
        Self::Parse,
        Self::Query,
    ];

    /// This stage's position in `ALL`, which is its declaration order.
    const fn slot(self) -> usize {
        self as usize
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Labels => "labels",
            Self::Layout => "layout",
            Self::Syntax => "syntax",
            Self::Detail => "detail",
            Self::Language => "language",
            Self::Projection => "projection",
            Self::Parse => "parse",
            Self::Query => "query",
        }
    }

    /// The enclosing measurement whose duration this stage's duration partitions.
    pub const fn parent(self) -> Parent {
        match self {
            Self::Labels | Self::Layout | Self::Syntax | Self::Detail => Parent::Preparation,
            Self::Language | Self::Projection | Self::Parse | Self::Query => Parent::Syntax,
        }
    }
}

/// A measurement partitioned by disjoint child stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parent {
    Preparation,
    Syntax,
}

impl Parent {
    /// Every parent, in report order.
    pub const ALL: [Self; 2] = [Self::Preparation, Self::Syntax];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Preparation => "preparation",
            Self::Syntax => "syntax",
        }
    }
}

pub enum Work {
    Hunk,
    Parse,
    Capture,
    ProjectedBytes(usize),
}

pub trait Observer: Sized {
    fn measure<T>(&mut self, stage: Stage, operation: impl FnOnce(&mut Self) -> T) -> T;
    fn count(&mut self, work: Work);
}

pub struct Unobserved;

impl Observer for Unobserved {
    #[inline]
    fn measure<T>(&mut self, _: Stage, operation: impl FnOnce(&mut Self) -> T) -> T {
        operation(self)
    }

    #[inline]
    fn count(&mut self, _: Work) {}
}

/// Accumulated stage durations and work counts from one observed preparation.
#[derive(Default)]
pub struct Profile {
    durations: [Duration; Stage::ALL.len()],
    work: WorkCounts,
}

impl Profile {
    pub fn duration(&self, stage: Stage) -> Duration {
        self.durations[stage.slot()]
    }

    pub fn work(&self) -> &WorkCounts {
        &self.work
    }
}

impl Observer for Profile {
    fn measure<T>(&mut self, stage: Stage, operation: impl FnOnce(&mut Self) -> T) -> T {
        let started = Instant::now();
        let result = operation(self);
        self.durations[stage.slot()] += started.elapsed();
        result
    }

    fn count(&mut self, work: Work) {
        match work {
            Work::Hunk => self.work.hunks += 1,
            Work::Parse => self.work.parse_calls += 1,
            Work::Capture => self.work.captures += 1,
            Work::ProjectedBytes(bytes) => self.work.projected_bytes += bytes,
        }
    }
}

#[derive(Default)]
pub struct WorkCounts {
    pub hunks: usize,
    pub parse_calls: usize,
    pub captures: usize,
    pub projected_bytes: usize,
}

#[cfg(test)]
mod tests {
    use super::Stage;

    #[test]
    fn lists_every_stage_at_its_own_slot() {
        for stage in Stage::ALL {
            assert_eq!(Stage::ALL[stage.slot()], stage);
        }
    }
}
