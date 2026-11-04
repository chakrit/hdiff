//! The streamed drivers, which read the input as the viewer does: `first-frame` times
//! launch to the first frame written, and `loading` times keys and hunks while
//! the worker still loads. Rules: docs/spec/performance.md#benchmark-drivers.

use std::{
    fmt::Write,
    io,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
    thread,
    time::{Duration, Instant},
};

use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::{Terminal, TerminalOptions, Viewport, backend::CrosstermBackend, layout::Size};

use super::MOVEMENT_AREA;
use crate::{
    file_tree::Arrival,
    input::{self, Opened, Origin},
    loading::{self, Channel, Message, Opening},
    measurement::Unobserved,
    prepared::{PreparedDocument, PreparedPage},
    terminal::{Answered, RunObserver, Tui},
    theme::ColorDepth,
};

/// How often the `loading` driver presses a key while files arrive: the key budget.
const KEY_INTERVAL: Duration = Duration::from_millis(1);

/// Why a streamed driver failed: the input could not be opened or read, viewer input
/// held no diff, or drawing failed.
pub enum Failure {
    Input(input::Error),
    NoDiff,
    Draw(io::Error),
}

/// The time from `started`, before the input opened, until the first frame is written.
pub fn first_frame(started: Instant, source: input::Source) -> Result<String, Failure> {
    let (channel, document, files) = open(source)?;
    let mut terminal = sink_terminal()?;
    let mut tui = Tui::start(document, Size::from(MOVEMENT_AREA), ColorDepth::TrueColor);
    tui.draw(&mut terminal).map_err(Failure::Draw)?;
    let elapsed = started.elapsed();
    drop(channel);

    Ok(format!(
        "first_frame version=1 ns={} files={files}\n",
        elapsed.as_nanos()
    ))
}

/// Opens the viewer on `source` while a key is pressed every [`KEY_INTERVAL`] until the
/// input ends, which ends the run once the key presser and the worker stop sending, then
/// reports how long each key waited for the frame that answered it and
/// how long each arrived update waited for the frame that showed it. Input without a file
/// loads nothing, so it reports no samples.
pub fn loading(source: input::Source) -> Result<String, Failure> {
    let (Channel { sender, receiver }, document, files) = open(source)?;
    if files == 0 {
        return Ok(Watch::default().report());
    }
    let mut terminal = sink_terminal()?;
    let mut tui = Tui::start(document, Size::from(MOVEMENT_AREA), ColorDepth::TrueColor);
    let mut watch = Watch::default();
    press_keys(sender, watch.sent.clone(), watch.ended.clone());

    tui.run_observed(&mut terminal, &receiver, &mut watch)
        .map_err(Failure::Draw)?;

    Ok(watch.report())
}

/// The channel the worker reads `source` into, with the document its first message opens
/// and how many files that document lists.
fn open(source: input::Source) -> Result<(Channel, PreparedDocument, usize), Failure> {
    let Opened { input, origin } = source.open().map_err(Failure::Input)?;
    let channel = loading::start(input);

    let document = match channel.opening() {
        Opening::Files(document) => *document,
        Opening::NoFiles(text) => match (text.is_empty(), origin) {
            (false, Origin::Stdin | Origin::PatchFile(_)) => return Err(Failure::NoDiff),
            (true, _) | (false, Origin::Comparison) => {
                PreparedDocument::Text(PreparedPage::of_text(&text, &mut Unobserved))
            }
        },
        Opening::Failed(error) => return Err(Failure::Input(origin.read_failure(error))),
    };
    let files = document.file_count();
    Ok((channel, document, files))
}

/// A terminal the size the movement driver draws at, writing to nowhere.
fn sink_terminal() -> Result<Terminal<CrosstermBackend<io::Sink>>, Failure> {
    let options = TerminalOptions {
        viewport: Viewport::Fixed(MOVEMENT_AREA),
    };

    Terminal::with_options(CrosstermBackend::new(io::sink()), options).map_err(Failure::Draw)
}

/// Presses `j` every [`KEY_INTERVAL`] on its own thread, recording when each press is
/// sent, until `ended` says the input has ended.
fn press_keys(sender: Sender<Message>, sent: Arc<Mutex<Vec<Instant>>>, ended: Arc<AtomicBool>) {
    thread::spawn(move || {
        while !ended.load(Ordering::Acquire) {
            sent.lock()
                .expect("no recording thread panics")
                .push(Instant::now());
            let down = Event::Key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
            if sender.send(Message::Terminal(Ok(down))).is_err() {
                return;
            }
            thread::sleep(KEY_INTERVAL);
        }
    });
}

/// What the `loading` driver observes: when each key was sent, which keys and updates
/// wait for the next frame, and how long each waited.
#[derive(Default)]
struct Watch {
    sent: Arc<Mutex<Vec<Instant>>>,
    ended: Arc<AtomicBool>,
    keys_answered: usize,
    input_ended: bool,
    keys_waiting: Vec<usize>,
    updates_waiting: Vec<Instant>,
    key_waits: Vec<Duration>,
    update_waits: Vec<Duration>,
}

impl RunObserver for Watch {
    fn answered(&mut self, answered: Answered) {
        match answered {
            Answered::Terminal => {
                if !self.input_ended {
                    self.keys_waiting.push(self.keys_answered);
                }
                self.keys_answered += 1;
            }
            Answered::Arrived(arrival) => {
                self.updates_waiting.push(Instant::now());
                if arrival == Arrival::Ended {
                    self.input_ended = true;
                    self.ended.store(true, Ordering::Release);
                }
            }
            Answered::Restyled => {}
        }
    }

    fn drew(&mut self) {
        let drawn = Instant::now();
        let sent = self.sent.lock().expect("no recording thread panics");

        self.key_waits.extend(
            self.keys_waiting
                .drain(..)
                .map(|key| drawn.duration_since(sent[key])),
        );
        self.update_waits.extend(
            self.updates_waiting
                .drain(..)
                .map(|received| drawn.duration_since(received)),
        );
    }
}

impl Watch {
    /// The report: a summary line, then one line per key answered while the input was
    /// arriving and one per update.
    fn report(&self) -> String {
        let mut report = format!(
            "loading version=1 keys={} updates={}\n",
            self.key_waits.len(),
            self.update_waits.len()
        );
        for wait in &self.key_waits {
            writeln!(report, "loading_key wait_ns={}", wait.as_nanos())
                .expect("writing to a String cannot fail");
        }
        for wait in &self.update_waits {
            writeln!(report, "update_frame wait_ns={}", wait.as_nanos())
                .expect("writing to a String cannot fail");
        }
        report
    }
}
