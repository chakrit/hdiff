//! `--bench <driver>` measurement. Contract: docs/spec/performance.md#benchmark-drivers.

use std::{
    ffi::OsStr,
    fmt::Write,
    io,
    time::{Duration, Instant},
};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{
    Terminal, TerminalOptions, Viewport,
    backend::CrosstermBackend,
    layout::{Rect, Size},
};

use crate::{
    input::InputSource,
    measurement::{self, Observer, Profile, Startup, Unobserved},
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
    pub fn run(self, source: InputSource) -> Result<String, String> {
        match self {
            Self::Prepare => {
                let (startup, prepared) = prepare(source, &mut Unobserved)?;
                Ok(preparation_report("preparation", &startup, &prepared))
            }
            Self::Stages => {
                let mut profile = Profile::default();
                let (startup, prepared) = prepare(source, &mut profile)?;

                let mut report = preparation_report("diagnostic", &startup, &prepared);
                report.push_str(&profile.report(startup.preparation));
                Ok(report)
            }
            Self::Movement => {
                let (_, prepared) = prepare(source, &mut Unobserved)?;
                movement(prepared).map_err(|error| error.to_string())
            }
        }
    }
}

fn prepare(
    source: InputSource,
    observer: &mut impl Observer,
) -> Result<(Startup, PreparedDocument), String> {
    let started = Instant::now();
    let input = source.read()?;
    let input_finished = Instant::now();
    let document = input.parse()?;
    let parse_finished = Instant::now();
    let prepared = PreparedDocument::prepare_observed(&document, observer);
    let ready = Instant::now();

    let startup = measurement::Startup {
        input: input_finished.duration_since(started),
        parse: parse_finished.duration_since(input_finished),
        preparation: ready.duration_since(parse_finished),
        ready: ready.duration_since(started),
        bytes: input.bytes().len(),
    };
    Ok((startup, prepared))
}

fn preparation_report(label: &str, startup: &Startup, prepared: &PreparedDocument) -> String {
    let mut report = format!(
        "{label} duration_ns={} files={} records={}\n",
        startup.preparation.as_nanos(),
        prepared.file_count(),
        prepared.record_count()
    );
    report.push_str(&startup.report());
    report
}

const MOVEMENT_AREA: Rect = Rect::new(0, 0, 120, 36);
const MOVEMENT_KEYS: [char; 4] = ['l', 'h', 'j', 'k'];
const MOVEMENT_ROUNDS: usize = 4;
const LAYOUT_COUNT: usize = 3;
const DETAIL_COUNT: usize = 2;
const WRAPPING_COUNT: usize = 2;

fn movement(prepared: PreparedDocument) -> io::Result<String> {
    let backend = CrosstermBackend::new(io::sink());
    let options = TerminalOptions {
        viewport: Viewport::Fixed(MOVEMENT_AREA),
    };
    let mut terminal = Terminal::with_options(backend, options)?;
    let mut tui = Tui::new(prepared, Size::from(MOVEMENT_AREA), u16::MAX);
    let mut report = String::new();

    tui.draw(&mut terminal)?;
    for _ in 0..WRAPPING_COUNT {
        for _ in 0..LAYOUT_COUNT {
            for _ in 0..DETAIL_COUNT {
                measure_movement(&mut tui, &mut terminal, &mut report)?;
                tui.step(&mut terminal, key_event('c'))?;
            }
            tui.step(&mut terminal, key_event('v'))?;
        }
        tui.step(&mut terminal, key_event('w'))?;
    }
    Ok(report)
}

/// Times each movement key under the current preferences, appending one line per step.
fn measure_movement<B: ratatui::backend::Backend>(
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

fn timed_step<B: ratatui::backend::Backend>(
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
