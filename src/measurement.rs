use std::time::{Duration, Instant};

/// A timed part of preparation, whose duration the profile accumulates. The stages
/// partition preparation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Labels,
    Layout,
    Detail,
    Styling,
}

impl Stage {
    /// Every stage, in report order, read from the one field each stage has in
    /// [`ByStage`], so a stage cannot exist without appearing here.
    pub const ALL: [Self; 4] = ByStage::STAGES.into_array();

    /// The stage's name in profile reports.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Labels => "labels",
            Self::Layout => "layout",
            Self::Detail => "detail",
            Self::Styling => "styling",
        }
    }
}

/// One unit of counted work done during preparation.
pub enum Work {
    Hunk,
    /// Payload bytes the scanner read.
    ScannedBytes(usize),
    /// Syntax spans the scanner gave styled records.
    Segments(usize),
}

/// What preparation reports its stages and work to.
pub trait Observer: Sized {
    /// What a stage's start leaves for its finish: the start time when the observer
    /// records stages, nothing when it does not.
    type Started;

    /// Marks a stage starting.
    fn start(&self) -> Self::Started;
    /// Marks `stage` finishing, from the mark its start left.
    fn finish(&mut self, stage: Stage, started: Self::Started);
    /// Records one unit of `work`.
    fn count(&mut self, work: Work);

    /// Runs `operation` as `stage`, giving it the observer to count the work inside it.
    fn measure<T>(&mut self, stage: Stage, operation: impl FnOnce(&mut Self) -> T) -> T {
        let started = self.start();
        let result = operation(self);
        self.finish(stage, started);
        result
    }

    /// Runs `operation` as `stage`, which counts no work, only time.
    fn time<T>(&mut self, stage: Stage, operation: impl FnOnce() -> T) -> T {
        let started = self.start();
        let result = operation();
        self.finish(stage, started);
        result
    }
}

/// An observer that records nothing, for preparation outside a profile.
pub struct Unobserved;

impl Observer for Unobserved {
    type Started = ();

    #[inline]
    fn start(&self) {}

    #[inline]
    fn finish(&mut self, _: Stage, (): ()) {}

    #[inline]
    fn count(&mut self, _: Work) {}
}

/// Accumulated stage durations and work counts from one observed preparation.
#[derive(Default)]
pub struct Profile {
    durations: ByStage<Duration>,
    work: WorkCounts,
}

impl Profile {
    /// The time spent in `stage`, summed over every time it ran.
    pub fn duration(&self, stage: Stage) -> Duration {
        *self.durations.get(stage)
    }

    pub fn work(&self) -> &WorkCounts {
        &self.work
    }
}

impl Observer for Profile {
    type Started = Instant;

    fn start(&self) -> Instant {
        Instant::now()
    }

    fn finish(&mut self, stage: Stage, started: Instant) {
        *self.durations.get_mut(stage) += started.elapsed();
    }

    fn count(&mut self, work: Work) {
        match work {
            Work::Hunk => self.work.hunks += 1,
            Work::ScannedBytes(bytes) => self.work.scanned_bytes += bytes,
            Work::Segments(segments) => self.work.segments += segments,
        }
    }
}

/// How much of each kind of [`Work`] one observed preparation did.
#[derive(Default)]
pub struct WorkCounts {
    pub hunks: usize,
    pub scanned_bytes: usize,
    pub segments: usize,
}

/// One value per stage. A new stage does not compile until it has a field here, a
/// `get` arm, a value in [`ByStage::STAGES`], and a place in [`ByStage::into_array`].
#[derive(Default)]
struct ByStage<T> {
    labels: T,
    layout: T,
    detail: T,
    styling: T,
}

impl ByStage<Stage> {
    /// Each field holding its own stage.
    const STAGES: Self = Self {
        labels: Stage::Labels,
        layout: Stage::Layout,
        detail: Stage::Detail,
        styling: Stage::Styling,
    };
}

impl<T: Copy> ByStage<T> {
    /// The values in report order.
    const fn into_array(self) -> [T; 4] {
        let Self {
            labels,
            layout,
            detail,
            styling,
        } = self;

        [labels, layout, detail, styling]
    }
}

impl<T> ByStage<T> {
    fn get(&self, stage: Stage) -> &T {
        match stage {
            Stage::Labels => &self.labels,
            Stage::Layout => &self.layout,
            Stage::Detail => &self.detail,
            Stage::Styling => &self.styling,
        }
    }

    fn get_mut(&mut self, stage: Stage) -> &mut T {
        match stage {
            Stage::Labels => &mut self.labels,
            Stage::Layout => &mut self.layout,
            Stage::Detail => &mut self.detail,
            Stage::Styling => &mut self.styling,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{ByStage, Stage};

    #[test]
    fn every_stage_holds_its_own_field() {
        let distinct = Stage::ALL
            .iter()
            .map(|stage| stage.label())
            .collect::<HashSet<_>>();

        assert_eq!(distinct.len(), Stage::ALL.len());
        for stage in Stage::ALL {
            assert_eq!(*ByStage::STAGES.get(stage), stage);
        }
    }
}
