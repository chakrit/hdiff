//! Streamed loading: a worker thread reads and prepares the input's files as they arrive,
//! one hunk at a time, and the main thread receives them with the terminal's
//! events over one channel. Rules: docs/spec/performance.md#preparation-boundary.

use std::{
    io::{self, BufRead},
    sync::mpsc::{self, Receiver, Sender},
    thread,
};

use crossterm::event::Event as TerminalEvent;

use crate::{
    display_lines::{self, DisplayLine},
    file_identity::FileIdentity,
    file_tree::Listing,
    measurement::Unobserved,
    parser::{Progress, SectionReader},
    prepared::{Arrived, FilePage, PreparedDocument, PreparedFiles, PreparedPage},
    source_text::SourceLine,
    syntax::DisplayScan,
};

/// What the main thread receives: a terminal event from the key reader, or what the
/// worker read.
pub enum Message {
    Terminal(io::Result<TerminalEvent>),
    Loaded(Loaded),
}

/// What the worker read from the input.
pub enum Loaded {
    /// The next update to the files read so far.
    Arrived(Arrived),
    /// The input ended without a file; it held only this text.
    NoFiles(Box<[SourceLine]>),
    /// Reading the input failed after the updates already sent.
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
    let worker = Worker {
        sender: sender.clone(),
        stopped: false,
    };
    thread::spawn(move || worker.read_files(input));

    Channel { sender, receiver }
}

/// What the input opens with, from the worker's first message.
pub enum Opening {
    /// The first file has arrived; more may follow.
    Files(Box<PreparedDocument>),
    /// The input ended without a file; it held only this text.
    NoFiles(Box<[SourceLine]>),
    /// Reading the input failed before any file arrived.
    Failed(io::Error),
}

impl Channel {
    /// What the input opens with; before a session starts, only the worker sends.
    pub fn opening(&self) -> Opening {
        let first = self.receiver.iter().find_map(|message| match message {
            Message::Loaded(loaded) => Some(loaded),
            Message::Terminal(_) => None,
        });

        match first.expect("the channel holds a sender while the receiver lives") {
            Loaded::Arrived(Arrived::Opened(page) | Arrived::Whole(page)) => Opening::Files(
                Box::new(PreparedDocument::Files(PreparedFiles::loading(page))),
            ),
            Loaded::Arrived(Arrived::Grew(_) | Arrived::Closed { .. } | Arrived::Ended(_)) => {
                unreachable!("the worker opens a file before it grows, closes or ends one")
            }
            Loaded::NoFiles(text) => Opening::NoFiles(text),
            Loaded::Failed(error) => Opening::Failed(error),
        }
    }
}

/// The worker's side of the channel, and whether the main thread has stopped receiving.
struct Worker {
    sender: Sender<Message>,
    stopped: bool,
}

/// The page of the file being read, with the syntax scan its lines continue and how many
/// of its lines are already sent.
struct OpenPage {
    scan: DisplayScan,
    sent: usize,
}

impl Worker {
    /// Reads `input` section by section, sending each file's page as its headers arrive
    /// and each hunk after them, then the text after the last file. Stops early
    /// when the main thread has stopped receiving.
    fn read_files(mut self, mut input: Box<dyn BufRead + Send>) {
        let mut reader = SectionReader::new(&mut input);
        let mut last_scan = None;

        loop {
            let mut open = None;
            let section = reader.next_section_reporting(&mut |progress| {
                self.report(&progress, &mut open);
            });
            if self.stopped {
                return;
            }
            let Some(section) = section else {
                break;
            };

            let view = section.view(&[]);
            let all_lines = display_lines::of_file_view(view);
            let listing = Listing::of(view);
            let update = match open {
                Some(OpenPage { mut scan, sent }) => {
                    let part = part_of(all_lines.skip(sent), &mut scan);
                    last_scan = Some(scan);
                    Arrived::Closed { part, listing }
                }
                None => {
                    let mut scan = DisplayScan::of_file(view.file);
                    let part = part_of(all_lines, &mut scan);
                    last_scan = Some(scan);
                    Arrived::Whole(FilePage::of_part(part, listing))
                }
            };
            self.send(Loaded::Arrived(update));
        }

        let ended = match (reader.finish(), last_scan) {
            (Ok(trailing), Some(mut scan)) => {
                let part = part_of(display_lines::of_text(&trailing), &mut scan);
                Loaded::Arrived(Arrived::Ended(part))
            }
            (Ok(trailing), None) => Loaded::NoFiles(trailing),
            (Err(error), _) => Loaded::Failed(error),
        };
        self.send(ended);
    }

    /// Sends the part of the open page `progress` reports: the page itself when the
    /// section's headers arrive, and the next hunk after them.
    fn report(&mut self, progress: &Progress<'_>, open: &mut Option<OpenPage>) {
        if self.stopped {
            return;
        }

        let mut count = 0;
        let lines = display_lines::of_progress(progress).inspect(|_| count += 1);
        let update = match (progress, open.as_mut()) {
            (Progress::Headers { git, headers, .. }, _) => {
                let mut scan = DisplayScan::of_headers(headers.as_ref());
                let part = part_of(lines, &mut scan);
                let listing = Listing::of_identity(&FileIdentity::of_unified(*git, headers));
                *open = Some(OpenPage { scan, sent: 0 });
                Arrived::Opened(FilePage::of_part(part, listing))
            }
            (Progress::Hunk { .. }, Some(page)) => Arrived::Grew(part_of(lines, &mut page.scan)),
            (Progress::Hunk { .. }, None) => unreachable!("a section reports its headers first"),
        };
        if let Some(page) = open.as_mut() {
            page.sent += count;
        }
        self.send(Loaded::Arrived(update));
    }

    /// Sends `loaded` to the main thread, noting when it has stopped receiving.
    fn send(&mut self, loaded: Loaded) {
        self.stopped = self.sender.send(Message::Loaded(loaded)).is_err();
    }
}

/// The page part `lines` make, read with `scan`.
fn part_of<'a>(
    lines: impl Iterator<Item = DisplayLine<'a>>,
    scan: &mut DisplayScan,
) -> PreparedPage {
    PreparedPage::of_lines(lines, scan, &mut Unobserved)
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::{Loaded, Message, start};
    use crate::{
        parser::parse_patch,
        prepared::{Arrived, PreparedDocument, PreparedFiles},
    };

    /// docs/spec/performance.md#preparation-boundary: pages that fill in as the input
    /// streams end up as the pages the whole input prepares, broken files and the text
    /// after the last file included.
    #[test]
    fn streamed_pages_are_the_pages_of_the_whole_input() {
        let patch = "intro\n--- a/a.go\n+++ b/a.go\n@@ -1,2 +1,2 @@\n x\n-/* y\n+z\n@@ -9 +9 @@\n-w */\n+v\nbetween\ndiff --git a/b.bin b/b.bin\nindex 1..2 100644\nBinary files a/b.bin and b/b.bin differ\n--- a/c.rs\n+++ b/c.rs\n@@ -1,2 +1,2 @@\n-fn c() {}\nbroken\ntrailer\n";
        let channel = start(Box::new(io::Cursor::new(patch.as_bytes().to_vec())));
        let mut updates = channel.receiver.iter().map(|message| match message {
            Message::Loaded(Loaded::Arrived(update)) => update,
            Message::Loaded(Loaded::NoFiles(_) | Loaded::Failed(_)) | Message::Terminal(_) => {
                panic!("the worker sends file updates")
            }
        });
        let Some(Arrived::Opened(first) | Arrived::Whole(first)) = updates.next() else {
            panic!("the first update opens a file");
        };
        let mut files = PreparedFiles::loading(first);
        for update in updates {
            let ended = matches!(update, Arrived::Ended(_));
            files.receive([update]);
            if ended {
                break;
            }
        }

        let streamed = PreparedDocument::Files(files);
        let whole = PreparedDocument::prepare(&parse_patch(patch.as_bytes()).expect("a diff"));
        assert_eq!(streamed.pages(), whole.pages());
        assert_eq!(
            streamed.file_selection(whole.page_order().first()),
            whole.file_selection(whole.page_order().first())
        );
    }
}
