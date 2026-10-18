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
    measurement::{Observer, Parent, Profile, Stage, Unobserved},
    parser::ParseError,
    preferences::ViewPreferences,
    prepared::PreparedDocument,
    terminal::Tui,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Driver {
    Prepare,
    Stages,
    Movement,
}

impl Driver {
    pub fn from_name(name: &OsStr) -> Option<Self> {
        match name.to_str()? {
            "prepare" => Some(Self::Prepare),
            "stages" => Some(Self::Stages),
            "movement" => Some(Self::Movement),
            _ => None,
        }
    }

    /// Reads, parses, and prepares `source`, runs the driver, and returns its report.
    pub fn run(self, source: input::Source) -> Result<String, Error> {
        match self {
            Self::Prepare => {
                let (startup, prepared) = prepare(source, &mut Unobserved)?;

                let mut report = String::new();
                write_preparation(&mut report, "preparation", &startup, &prepared);
                Ok(report)
            }
            Self::Stages => {
                let mut profile = Profile::default();
                let (startup, prepared) = prepare(source, &mut profile)?;

                let mut report = String::new();
                write_preparation(&mut report, "diagnostic", &startup, &prepared);
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
            Self::Input(error) => error.fmt(formatter),
            Self::Parse(error) => write!(formatter, "{error}"),
            Self::Terminal(error) => error.fmt(formatter),
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

fn write_preparation(
    report: &mut String,
    record: &str,
    startup: &Startup,
    prepared: &PreparedDocument,
) {
    writeln!(
        report,
        "{record} duration_ns={} files={} records={}",
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
const MOVEMENT_KEYS: [char; 4] = ['l', 'h', 'j', 'k'];
const MOVEMENT_ROUNDS: usize = 4;

fn movement(prepared: PreparedDocument) -> io::Result<String> {
    let backend = CrosstermBackend::new(io::sink());
    let options = TerminalOptions {
        viewport: Viewport::Fixed(MOVEMENT_AREA),
    };
    let mut terminal = Terminal::with_options(backend, options)?;
    let mut tui = Tui::new(
        prepared,
        Size::from(MOVEMENT_AREA),
        crate::theme::ColorDepth::TrueColor,
    );
    let mut report = String::new();

    tui.draw(&mut terminal)?;
    visit_every_mode(
        &mut tui,
        &mut terminal,
        'w',
        |p| p.wrapping,
        &mut |tui, terminal| {
            visit_every_mode(tui, terminal, 'v', |p| p.layout, &mut |tui, terminal| {
                visit_every_mode(
                    tui,
                    terminal,
                    'c',
                    |p| p.granularity,
                    &mut |tui, terminal| measure_movement(tui, terminal, &mut report),
                )
            })
        },
    )?;
    Ok(report)
}

/// Runs `visit` once per mode that `key` cycles through, pressing `key` after each visit
/// until `mode` returns to where it started. Deriving the visits from the cycle covers
/// every mode the interaction offers without restating how many there are.
fn visit_every_mode<B: Backend, M: PartialEq>(
    tui: &mut Tui,
    terminal: &mut Terminal<B>,
    key: char,
    mode: fn(ViewPreferences) -> M,
    visit: &mut impl FnMut(&mut Tui, &mut Terminal<B>) -> Result<(), B::Error>,
) -> Result<(), B::Error> {
    let start = mode(tui.preferences());

    loop {
        visit(tui, terminal)?;
        tui.step(terminal, key_event(key))?;
        if mode(tui.preferences()) == start {
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

    for key in MOVEMENT_KEYS
        .iter()
        .cycle()
        .take(MOVEMENT_KEYS.len() * MOVEMENT_ROUNDS)
    {
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

fn key_event(key: char) -> Event {
    Event::Key(KeyEvent::new(KeyCode::Char(key), KeyModifiers::NONE))
}
