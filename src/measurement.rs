use std::time::{Duration, Instant};

#[derive(Clone, Copy)]
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

#[derive(Default)]
pub struct Profile {
    durations: [Duration; 8],
    hunks: usize,
    parse_calls: usize,
    captures: usize,
    projected_bytes: usize,
}

impl Observer for Profile {
    fn measure<T>(&mut self, stage: Stage, operation: impl FnOnce(&mut Self) -> T) -> T {
        let started = Instant::now();
        let result = operation(self);
        self.durations[stage as usize] += started.elapsed();
        result
    }

    fn count(&mut self, work: Work) {
        match work {
            Work::Hunk => self.hunks += 1,
            Work::Parse => self.parse_calls += 1,
            Work::Capture => self.captures += 1,
            Work::ProjectedBytes(bytes) => self.projected_bytes += bytes,
        }
    }
}

impl Profile {
    pub fn report(&self, preparation: Duration) -> String {
        let mut output = String::from("profile version=1\n");
        let groups = [
            (
                "preparation",
                preparation,
                &[
                    ("labels", Stage::Labels),
                    ("layout", Stage::Layout),
                    ("syntax", Stage::Syntax),
                    ("detail", Stage::Detail),
                ][..],
            ),
            (
                "syntax",
                self.durations[Stage::Syntax as usize],
                &[
                    ("language", Stage::Language),
                    ("projection", Stage::Projection),
                    ("parse", Stage::Parse),
                    ("query", Stage::Query),
                ][..],
            ),
        ];
        for (parent, total, stages) in groups {
            let mut attributed = Duration::ZERO;
            for (name, stage) in stages {
                let duration = self.durations[*stage as usize];
                attributed += duration;
                output.push_str(&format!(
                    "stage name={name} parent={parent} duration_ns={}\n",
                    duration.as_nanos()
                ));
            }
            let remainder = total
                .checked_sub(attributed)
                .expect("disjoint stage durations fit inside their parent");
            output.push_str(&format!(
                "stage name=unattributed parent={parent} duration_ns={}\n",
                remainder.as_nanos()
            ));
        }
        output.push_str(&format!(
            "work hunks={} parse_calls={} captures={} projected_bytes={}\n",
            self.hunks, self.parse_calls, self.captures, self.projected_bytes
        ));
        output
    }
}

pub struct Startup {
    pub input: Duration,
    pub parse: Duration,
    pub preparation: Duration,
    pub ready: Duration,
    pub bytes: usize,
}

impl Startup {
    pub fn report(&self) -> String {
        format!(
            "startup version=1 input_ns={} parse_ns={} preparation_ns={} ready_ns={} bytes={}\n",
            self.input.as_nanos(),
            self.parse.as_nanos(),
            self.preparation.as_nanos(),
            self.ready.as_nanos(),
            self.bytes
        )
    }
}
