//! Streamed loading: a worker thread reads and prepares the input's files as they arrive,
//! and the main thread receives them with the terminal's events over one channel. Rules:
//! docs/spec/performance.md#preparation-boundary.

use std::{
    io::{self, BufRead},
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use crossterm::event::Event as TerminalEvent;

use crate::{
    document::FileSection, file_tree::Arrival, parser::SectionReader, prepared::FilePage,
    source_text::SourceLine,
};

/// What the main thread receives: a terminal event from the key reader, or what the
/// worker read.
pub enum Message {
    Terminal(io::Result<TerminalEvent>),
    Loaded(Loaded),
}

/// What the worker read from the input.
pub enum Loaded {
    /// Prepared files in input order, with whether more input is coming.
    Files {
        pages: Vec<FilePage>,
        arrival: Arrival,
    },
    /// The input ended without a file; it held only this text.
    NoFiles(Box<[SourceLine]>),
    /// Reading the input failed after the files already sent.
    Failed(io::Error),
}

/// The channel the worker and the key reader send on and the main thread receives on.
pub struct Channel {
    pub sender: Sender<Message>,
    pub receiver: Receiver<Message>,
}

/// Starts the worker reading `input` on its own thread; its files and its end arrive on
/// the returned channel.
pub fn start(input: Box<dyn BufRead + Send>) -> Channel {
    let (sender, receiver) = mpsc::channel();
    let worker_sender = sender.clone();
    thread::spawn(move || read_files(input, &worker_sender));

    Channel { sender, receiver }
}

/// Reads `input` one section at a time and sends each file's prepared page. A file is
/// sent once the next one arrives, because the last file's page shows the text after
/// it. Stops early when the main thread has stopped receiving.
fn read_files(mut input: Box<dyn BufRead + Send>, sender: &Sender<Message>) {
    let mut reader = SectionReader::new(&mut input);
    let mut held: Option<FileSection> = None;

    while let Some(section) = reader.next_section() {
        if let Some(previous) = held.replace(section) {
            let page = FilePage::prepare(previous.view(&[]));
            if !send(sender, files(vec![page], Arrival::More)) {
                return;
            }
        }
    }

    let ended = match (reader.finish(), held) {
        (Ok(trailing), Some(last)) => files(
            vec![FilePage::prepare(last.view(&trailing))],
            Arrival::Ended,
        ),
        (Ok(trailing), None) => Loaded::NoFiles(trailing),
        (Err(error), Some(last)) => {
            if !send(
                sender,
                files(vec![FilePage::prepare(last.view(&[]))], Arrival::More),
            ) {
                return;
            }
            Loaded::Failed(error)
        }
        (Err(error), None) => Loaded::Failed(error),
    };
    send(sender, ended);
}

/// `pages` with whether more input is coming.
fn files(pages: Vec<FilePage>, arrival: Arrival) -> Loaded {
    Loaded::Files { pages, arrival }
}

/// Sends `loaded` to the main thread; false once it has stopped receiving.
fn send(sender: &Sender<Message>, loaded: Loaded) -> bool {
    sender.send(Message::Loaded(loaded)).is_ok()
}
