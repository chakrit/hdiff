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
    /// Every stage, in report order, read from the one field each stage has in
    /// [`ByStage`], so a stage cannot exist without appearing here.
    pub const ALL: [Self; 8] = ByStage::STAGES.into_array();

    /// The stages grouped under their parents, in report order.
    pub fn families() -> impl Iterator<Item = (Parent, &'static [Stage])> {
        Self::ALL
            .chunk_by(|stage, next| stage.parent() == next.parent())
            .map(|stages| (stages[0].parent(), stages))
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
    durations: ByStage<Duration>,
    work: WorkCounts,
}

impl Profile {
    pub fn duration(&self, stage: Stage) -> Duration {
        *self.durations.get(stage)
    }

    pub fn work(&self) -> &WorkCounts {
        &self.work
    }
}

impl Observer for Profile {
    fn measure<T>(&mut self, stage: Stage, operation: impl FnOnce(&mut Self) -> T) -> T {
        let started = Instant::now();
        let result = operation(self);
        *self.durations.get_mut(stage) += started.elapsed();
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
    };
}

impl<T: Copy> ByStage<T> {
    /// The values in report order.
    const fn into_array(self) -> [T; 8] {
        let Self {
            labels,
            layout,
            syntax,
            detail,
            language,
            projection,
            parse,
            query,
        } = self;

        [
            labels, layout, syntax, detail, language, projection, parse, query,
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
    fn each_parent_owns_one_contiguous_family() {
        let parents = Stage::families()
            .map(|(parent, _)| parent)
            .collect::<Vec<_>>();

        assert_eq!(parents, [Parent::Preparation, Parent::Syntax]);
    }
}
