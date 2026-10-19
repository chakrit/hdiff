//! `--bench <driver>` measurement. Rules: docs/spec/performance.md#benchmark-drivers.

use std::{
    ffi::OsStr,
    fmt::{self, Write},
    io,
    time::{Duration, Instant},
};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Terminal, TerminalOptions, Viewport,
    backend::{Backend, CrosstermBackend},
    layout::{Rect, Size},
};

use crate::{
    document::DiffDocument,
    input,
    measurement::{Family, Observer, Parent, Profile, Stage, Unobserved},
    navigation::{interaction::Command, scroll::Movement},
    preferences::PreferenceChange,
    prepared::PreparedDocument,
    terminal::{self, Tui},
    theme::ColorDepth,
};

/// What a `--bench` run measures: preparation, preparation by stage, or movement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Driver {
    Prepare,
    Stages,
    Movement,
}

impl Driver {
    /// Every driver, in the order usage messages list them.
    pub const ALL: [Self; 3] = [Self::Prepare, Self::Stages, Self::Movement];

    /// The name `--bench` takes for the driver.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Prepare => "prepare",
            Self::Stages => "stages",
            Self::Movement => "movement",
        }
    }

    /// The driver `name` names, if any.
    pub fn of_name(name: &OsStr) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|driver| OsStr::new(driver.label()) == name)
    }

    /// The driver names as an English list, such as `prepare, stages, or movement`.
    pub fn choices() -> String {
        let [leading @ .., last] = Self::ALL.map(Self::label);

        format!("{}, or {last}", leading.join(", "))
    }

    /// Prepares `document`, runs the driver, and returns its report. `clock` has timed
    /// reading and parsing the input the document came from.
    pub fn run(self, clock: PreparationClock, document: &DiffDocument) -> io::Result<String> {
        match self {
            Self::Prepare => {
                let Ready { startup, prepared } = clock.prepare(document, &mut Unobserved);

                Ok(preparation_report(
                    PreparationRecord::Preparation,
                    &startup,
                    &prepared,
                ))
            }
            Self::Stages => {
                let mut profile = Profile::default();
                let Ready { startup, prepared } = clock.prepare(document, &mut profile);

                let mut report =
                    preparation_report(PreparationRecord::Diagnostic, &startup, &prepared);
                write_profile(&mut report, &profile, startup.preparation);
                Ok(report)
            }
            Self::Movement => {
                let Ready { prepared, .. } = clock.prepare(document, &mut Unobserved);
                movement(prepared)
            }
        }
    }
}

// Startup timing

/// Times startup from the moment reading the input begins. Each later phase is marked by
/// consuming the clock for the phase before it, so phases can only be marked in order.
pub struct InputClock {
    started: Instant,
}

/// A startup clock whose input has been read.
pub struct ParseClock {
    reading: InputClock,
    input_finished: Instant,
    bytes: usize,
}

/// A startup clock whose input has been read and parsed.
pub struct PreparationClock {
    parsing: ParseClock,
    parse_finished: Instant,
}

impl InputClock {
    /// A clock started now, before input is read.
    pub fn start() -> Self {
        Self {
            started: Instant::now(),
        }
    }

    /// Marks reading `content` finished.
    pub fn input_read(self, content: &input::Content) -> ParseClock {
        ParseClock {
            reading: self,
            input_finished: Instant::now(),
            bytes: content.bytes().len(),
        }
    }
}

impl ParseClock {
    /// Marks parsing finished.
    pub fn parsed(self) -> PreparationClock {
        PreparationClock {
            parsing: self,
            parse_finished: Instant::now(),
        }
    }
}

impl PreparationClock {
    /// Prepares `document` under `observer`, timing each startup phase.
    fn prepare(self, document: &DiffDocument, observer: &mut impl Observer) -> Ready {
        let prepared = PreparedDocument::prepare_observed(document, observer);
        let ready = Instant::now();

        let ParseClock {
            reading: InputClock { started },
            input_finished,
            bytes,
        } = self.parsing;
        let startup = Startup {
            input: input_finished.duration_since(started),
            parse: self.parse_finished.duration_since(input_finished),
            preparation: ready.duration_since(self.parse_finished),
            ready: ready.duration_since(started),
            bytes,
        };
        Ready { startup, prepared }
    }
}

/// A prepared document with the startup timing that led to it.
struct Ready {
    startup: Startup,
    prepared: PreparedDocument,
}

/// Durations from the start of the run until the prepared document is ready, and the
/// size of the input read.
struct Startup {
    input: Duration,
    parse: Duration,
    preparation: Duration,
    ready: Duration,
    bytes: usize,
}

// Reports

/// The first report line's record name: plain preparation, or preparation under a
/// profile.
enum PreparationRecord {
    Preparation,
    Diagnostic,
}

impl PreparationRecord {
    /// The record name the report line opens with.
    fn label(self) -> &'static str {
        match self {
            Self::Preparation => Parent::Preparation.label(),
            Self::Diagnostic => "diagnostic",
        }
    }
}

/// The preparation record and the startup record.
fn preparation_report(
    record: PreparationRecord,
    startup: &Startup,
    prepared: &PreparedDocument,
) -> String {
    let mut report = String::new();
    push_line(
        &mut report,
        format_args!(
            "{} duration_ns={} files={} records={}",
            record.label(),
            startup.preparation.as_nanos(),
            prepared.file_count(),
            prepared.record_count()
        ),
    );
    push_line(
        &mut report,
        format_args!(
            "startup version=1 input_ns={} parse_ns={} preparation_ns={} ready_ns={} bytes={}",
            startup.input.as_nanos(),
            startup.parse.as_nanos(),
            startup.preparation.as_nanos(),
            startup.ready.as_nanos(),
            startup.bytes
        ),
    );
    report
}

/// The profile record, each parent's stage records with its unattributed remainder, and
/// the work counts.
fn write_profile(report: &mut String, profile: &Profile, preparation: Duration) {
    push_line(report, format_args!("profile version=1"));

    for Family { parent, stages } in Stage::families() {
        let total = match parent.measured_by() {
            None => preparation,
            Some(stage) => profile.duration(stage),
        };

        let attributed = stages
            .iter()
            .map(|&stage| profile.duration(stage))
            .sum::<Duration>();
        for &stage in stages {
            write_stage(report, stage.label(), parent, profile.duration(stage));
        }

        let unattributed = total
            .checked_sub(attributed)
            .expect("disjoint stage durations fit inside their parent");
        write_stage(report, "unattributed", parent, unattributed);
    }

    let work = profile.work();
    push_line(
        report,
        format_args!(
            "work hunks={} parse_calls={} captures={} projected_bytes={}",
            work.hunks, work.parse_calls, work.captures, work.projected_bytes
        ),
    );
}

/// One stage record under `parent`.
fn write_stage(report: &mut String, name: &str, parent: Parent, duration: Duration) {
    push_line(
        report,
        format_args!(
            "stage name={name} parent={} duration_ns={}",
            parent.label(),
            duration.as_nanos()
        ),
    );
}

/// Appends `line` and a newline to `report`.
fn push_line(report: &mut String, line: fmt::Arguments<'_>) {
    writeln!(report, "{line}").expect("writing to a String cannot fail");
}

// Movement

/// The screen the movement driver draws to.
const MOVEMENT_AREA: Rect = Rect::new(0, 0, 120, 36);
/// The movements the driver times in each mode, in order.
const MOVEMENTS: [Movement; 4] = [
    Movement::Right,
    Movement::Left,
    Movement::Down,
    Movement::Up,
];
/// How many times each mode repeats its movements.
const MOVEMENT_ROUNDS: usize = 4;

/// Every preference change the movement driver cycles through, outermost first.
const MODE_CYCLES: [PreferenceChange; 3] = [
    PreferenceChange::ToggleWrapping,
    PreferenceChange::NextLayout,
    PreferenceChange::ToggleGranularity,
];

/// The movement report: each movement key's step time in every mode, drawn to a
/// discarded terminal.
fn movement(prepared: PreparedDocument) -> io::Result<String> {
    let backend = CrosstermBackend::new(io::sink());
    let options = TerminalOptions {
        viewport: Viewport::Fixed(MOVEMENT_AREA),
    };
    let mut terminal = Terminal::with_options(backend, options)?;
    let mut tui = Tui::new(prepared, Size::from(MOVEMENT_AREA), ColorDepth::TrueColor);
    let mut report = String::new();

    tui.draw(&mut terminal)?;
    visit_modes(&mut tui, &mut terminal, &MODE_CYCLES, &mut report)?;
    Ok(report)
}

/// Measures movement once per combination of the modes `cycles` step through. Each cycle
/// presses its key after every visit until the preferences return to where they started.
/// Every inner cycle has come back around by then, so the visits cover every mode without
/// restating how many there are or which preference each change moves.
fn visit_modes<B: Backend>(
    tui: &mut Tui,
    terminal: &mut Terminal<B>,
    cycles: &[PreferenceChange],
    report: &mut String,
) -> Result<(), B::Error> {
    let [change, inner @ ..] = cycles else {
        return measure_movement(tui, terminal, report);
    };
    let key = bound_character(Command::Preference(*change));
    let start = tui.preferences();

    loop {
        visit_modes(tui, terminal, inner, report)?;
        let before = tui.preferences();
        tui.step(terminal, key_event(key))?;

        assert_ne!(
            tui.preferences(),
            before,
            "{key} must change the preference it cycles"
        );
        if tui.preferences() == start {
            return Ok(());
        }
    }
}

/// Times each movement key under the current preferences, appending one line per step.
fn measure_movement<B: Backend>(
    tui: &mut Tui,
    terminal: &mut Terminal<B>,
    report: &mut String,
) -> Result<(), B::Error> {
    let preferences = tui.preferences();
    let keys = MOVEMENTS.map(|movement| bound_character(Command::Movement(movement)));

    for key in keys.iter().cycle().take(keys.len() * MOVEMENT_ROUNDS) {
        let step = timed_step(tui, terminal, *key)?;
        push_line(
            report,
            format_args!(
                "movement version=2 layout={} detail={} wrap={} key={key} step_ns={}",
                preferences.layout.label(),
                preferences.granularity.label(),
                preferences.wrapping.label(),
                step.as_nanos()
            ),
        );
    }
    Ok(())
}

/// How long one step takes to answer `key` and draw.
fn timed_step<B: Backend>(
    tui: &mut Tui,
    terminal: &mut Terminal<B>,
    key: char,
) -> Result<Duration, B::Error> {
    let started = Instant::now();
    tui.step(terminal, key_event(key))?;
    Ok(started.elapsed())
}

/// The character key bound to `command`, which the driver presses for it.
fn bound_character(command: Command) -> char {
    terminal::character_for(command).expect("every driven command has a character binding")
}

/// The terminal event of pressing `key` alone.
fn key_event(key: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE))
}
