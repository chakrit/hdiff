use std::time::{Duration, Instant};

/// A timed part of preparation, whose duration the profile accumulates.
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
    Scan,
}

impl Stage {
    /// Every stage, in report order, read from the one field each stage has in
    /// [`ByStage`], so a stage cannot exist without appearing here.
    pub const ALL: [Self; 9] = ByStage::STAGES.into_array();

    /// The stage's name in profile reports.
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
            Self::Scan => "scan",
        }
    }

    /// The enclosing measurement whose duration this stage's duration partitions.
    pub const fn parent(self) -> Parent {
        match self {
            Self::Labels | Self::Layout | Self::Syntax | Self::Detail => Parent::Preparation,
            Self::Language | Self::Projection | Self::Parse | Self::Query | Self::Scan => {
                Parent::Syntax
            }
        }
    }
}

/// A measurement partitioned by disjoint child stages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Parent {
    /// The whole preparation, timed outside the profile.
    Preparation,
    /// Syntax highlighting, timed as [`Stage::Syntax`].
    Syntax,
}

impl Parent {
    /// Every parent, in report order.
    pub const ALL: [Self; 2] = [Self::Preparation, Self::Syntax];

    /// The stages that partition this parent's duration, in report order.
    pub fn stages(self) -> impl Iterator<Item = Stage> {
        Stage::ALL
            .into_iter()
            .filter(move |stage| stage.parent() == self)
    }

    /// The parent's name in profile reports.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Preparation => "preparation",
            Self::Syntax => Stage::Syntax.label(),
        }
    }

    /// The profiled stage whose duration is this parent's total; none for a parent timed
    /// outside the profile.
    pub const fn measured_by(self) -> Option<Stage> {
        match self {
            Self::Preparation => None,
            Self::Syntax => Some(Stage::Syntax),
        }
    }
}

/// One unit of counted work done during preparation.
pub enum Work {
    Hunk,
    ParseCall,
    Capture,
    ProjectedBytes(usize),
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
            Work::ParseCall => self.work.parse_calls += 1,
            Work::Capture => self.work.captures += 1,
            Work::ProjectedBytes(bytes) => self.work.projected_bytes += bytes,
        }
    }
}

/// How much of each kind of [`Work`] one observed preparation did.
#[derive(Default)]
pub struct WorkCounts {
    pub hunks: usize,
    pub parse_calls: usize,
    pub captures: usize,
    pub projected_bytes: usize,
}

/// One value per stage. A new stage does not compile until it has a field here, a
/// `get` arm, a value in [`ByStage::STAGES`], and a place in [`ByStage::into_array`].
#[derive(Default)]
struct ByStage<T> {
    labels: T,
    layout: T,
    syntax: T,
    detail: T,
    language: T,
    projection: T,
    parse: T,
    query: T,
    scan: T,
}

impl ByStage<Stage> {
    /// Each field holding its own stage.
    const STAGES: Self = Self {
        labels: Stage::Labels,
        layout: Stage::Layout,
        syntax: Stage::Syntax,
        detail: Stage::Detail,
        language: Stage::Language,
        projection: Stage::Projection,
        parse: Stage::Parse,
        query: Stage::Query,
        scan: Stage::Scan,
    };
}

impl<T: Copy> ByStage<T> {
    /// The values in report order.
    const fn into_array(self) -> [T; 9] {
        let Self {
            labels,
            layout,
            syntax,
            detail,
            language,
            projection,
            parse,
            query,
            scan,
        } = self;

        [
            labels, layout, syntax, detail, language, projection, parse, query, scan,
        ]
    }
}

impl<T> ByStage<T> {
    fn get(&self, stage: Stage) -> &T {
        match stage {
            Stage::Labels => &self.labels,
            Stage::Layout => &self.layout,
            Stage::Syntax => &self.syntax,
            Stage::Detail => &self.detail,
            Stage::Language => &self.language,
            Stage::Projection => &self.projection,
            Stage::Parse => &self.parse,
            Stage::Query => &self.query,
            Stage::Scan => &self.scan,
        }
    }

    fn get_mut(&mut self, stage: Stage) -> &mut T {
        match stage {
            Stage::Labels => &mut self.labels,
            Stage::Layout => &mut self.layout,
            Stage::Syntax => &mut self.syntax,
            Stage::Detail => &mut self.detail,
            Stage::Language => &mut self.language,
            Stage::Projection => &mut self.projection,
            Stage::Parse => &mut self.parse,
            Stage::Query => &mut self.query,
            Stage::Scan => &mut self.scan,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{ByStage, Parent, Stage};

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

    #[test]
    fn reports_every_stage_once_under_its_parent_in_stage_order() {
        let reported = Parent::ALL
            .into_iter()
            .flat_map(Parent::stages)
            .collect::<Vec<_>>();

        assert_eq!(reported, Stage::ALL);
    }
}
