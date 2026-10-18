//! `--bench <driver>` measurement. Contract: docs/spec/performance.md#benchmark-drivers.

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
    input,
    interaction::{Command, Movement},
    measurement::{Observer, Parent, Profile, Stage, Unobserved},
    parser::ParseError,
    preferences::PreferenceChange,
    prepared::PreparedDocument,
    terminal::{self, Tui},
    theme::ColorDepth,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Driver {
    Prepare,
    Stages,
    Movement,
}

impl Driver {
    /// Every driver, in the order usage messages list them.
    pub const ALL: [Self; 3] = [Self::Prepare, Self::Stages, Self::Movement];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Prepare => "prepare",
            Self::Stages => "stages",
            Self::Movement => "movement",
        }
    }

    pub fn from_name(name: &OsStr) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|driver| OsStr::new(driver.label()) == name)
    }

    /// The driver names as an English list, such as `prepare, stages, or movement`.
    pub fn choices() -> String {
        let labels = Self::ALL.map(Self::label);
        let (last, leading) = labels.split_last().expect("at least one driver");

        format!("{}, or {last}", leading.join(", "))
    }

    /// Reads, parses, and prepares `source`, runs the driver, and returns its report.
    pub fn run(self, source: input::Source) -> Result<String, Error> {
        match self {
            Self::Prepare => {
                let (startup, prepared) = prepare(source, &mut Unobserved)?;

                let mut report = String::new();
                write_preparation(
                    &mut report,
                    PreparationRecord::Preparation,
                    &startup,
                    &prepared,
                );
                Ok(report)
            }
            Self::Stages => {
                let mut profile = Profile::default();
                let (startup, prepared) = prepare(source, &mut profile)?;

                let mut report = String::new();
                write_preparation(
                    &mut report,
                    PreparationRecord::Diagnostic,
                    &startup,
                    &prepared,
                );
                write_profile(&mut report, &profile, startup.preparation);
                Ok(report)
            }
            Self::Movement => {
                let (_, prepared) = prepare(source, &mut Unobserved)?;
                movement(prepared).map_err(Error::Terminal)
            }
        }
    }
}

/// Why a driver produced no report.
#[derive(Debug)]
pub enum Error {
    Input(input::Error),
    Parse(ParseError),
    Terminal(io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input(error) => write!(formatter, "{error}"),
            Self::Parse(error) => write!(formatter, "{error}"),
            Self::Terminal(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for Error {}

/// Durations from the start of the run until the prepared document is ready.
struct Startup {
    input: Duration,
    parse: Duration,
    preparation: Duration,
    ready: Duration,
    bytes: usize,
}

fn prepare(
    source: input::Source,
    observer: &mut impl Observer,
) -> Result<(Startup, PreparedDocument), Error> {
    let started = Instant::now();

    let content = source.read().map_err(Error::Input)?;
    let input_finished = Instant::now();
    let bytes = content.bytes().len();
    let document = content.parse().map_err(Error::Parse)?;
    let parse_finished = Instant::now();
    let prepared = PreparedDocument::prepare_observed(&document, observer);
    let ready = Instant::now();

    let startup = Startup {
        input: input_finished.duration_since(started),
        parse: parse_finished.duration_since(input_finished),
        preparation: ready.duration_since(parse_finished),
        ready: ready.duration_since(started),
        bytes,
    };
    Ok((startup, prepared))
}

// reports

/// The first report line's record name: plain preparation, or preparation under a profile.
#[derive(Clone, Copy)]
enum PreparationRecord {
    Preparation,
    Diagnostic,
}

impl PreparationRecord {
    fn label(self) -> &'static str {
        match self {
            Self::Preparation => "preparation",
            Self::Diagnostic => "diagnostic",
        }
    }
}

fn write_preparation(
    report: &mut String,
    record: PreparationRecord,
    startup: &Startup,
    prepared: &PreparedDocument,
) {
    writeln!(
        report,
        "{} duration_ns={} files={} records={}",
        record.label(),
        startup.preparation.as_nanos(),
        prepared.file_count(),
        prepared.record_count()
    )
    .expect("writing to a String cannot fail");
    writeln!(
        report,
        "startup version=1 input_ns={} parse_ns={} preparation_ns={} ready_ns={} bytes={}",
        startup.input.as_nanos(),
        startup.parse.as_nanos(),
        startup.preparation.as_nanos(),
        startup.ready.as_nanos(),
        startup.bytes
    )
    .expect("writing to a String cannot fail");
}

fn write_profile(report: &mut String, profile: &Profile, preparation: Duration) {
    report.push_str("profile version=1\n");

    for parent in Parent::ALL {
        let total = match parent {
            Parent::Preparation => preparation,
            Parent::Syntax => profile.duration(Stage::Syntax),
        };
        let children = Stage::ALL
            .into_iter()
            .filter(|stage| stage.parent() == parent);

        let mut attributed = Duration::ZERO;
        for stage in children {
            let duration = profile.duration(stage);
            attributed += duration;
            write_stage(report, stage.label(), parent, duration);
        }

        let unattributed = total
            .checked_sub(attributed)
            .expect("disjoint stage durations fit inside their parent");
        write_stage(report, "unattributed", parent, unattributed);
    }

    let work = profile.work();
    writeln!(
        report,
        "work hunks={} parse_calls={} captures={} projected_bytes={}",
        work.hunks, work.parse_calls, work.captures, work.projected_bytes
    )
    .expect("writing to a String cannot fail");
}

fn write_stage(report: &mut String, name: &str, parent: Parent, duration: Duration) {
    writeln!(
        report,
        "stage name={name} parent={} duration_ns={}",
        parent.label(),
        duration.as_nanos()
    )
    .expect("writing to a String cannot fail");
}

// movement

const MOVEMENT_AREA: Rect = Rect::new(0, 0, 120, 36);
const MOVEMENTS: [Movement; 4] = [
    Movement::Right,
    Movement::Left,
    Movement::Down,
    Movement::Up,
];
const MOVEMENT_ROUNDS: usize = 4;

/// Every preference change the movement driver cycles through, outermost first.
const MODE_CYCLES: [PreferenceChange; 3] = [
    PreferenceChange::ToggleWrapping,
    PreferenceChange::NextLayout,
    PreferenceChange::ToggleGranularity,
];

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
        writeln!(
            report,
            "movement version=2 layout={} detail={} wrap={} key={key} step_ns={}",
            preferences.layout.label(),
            preferences.granularity.label(),
            preferences.wrapping.label(),
            step.as_nanos()
        )
        .expect("writing to a String cannot fail");
    }
    Ok(())
}

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

fn key_event(key: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE))
}
