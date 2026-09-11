use std::time::Instant;

use ratatui::{Terminal, backend::TestBackend};

use crate::{
    interaction::{
        DiffGranularity, DiffLayout, Input, Interaction, Transition, ViewPreferences, Viewport,
        transition_interaction,
    },
    parser::parse_unified_diff,
    prepared::PreparedDocument,
};

use super::{navigation_bounds, view::render_prepared_frame};

// Diagnostic timings, not a correctness test or a terminal transport measurement.
// The measurement contract is recorded in docs/spec/performance.md.
#[test]
#[ignore = "run explicitly in release mode to measure movement costs"]
fn measures_prepared_movement() {
    for hunks in [16, 64, 256, 1024] {
        let patch = rust_patch(hunks);
        let document = parse_unified_diff(patch.as_bytes()).expect("valid measurement patch");
        let started = Instant::now();
        let prepared = PreparedDocument::prepare(&document);
        let preparation_elapsed = started.elapsed();
        println!(
            "movement_preparation hunks_per_file={hunks} files={} records={} duration_ns={}",
            prepared.file_count(),
            prepared.record_count(),
            preparation_elapsed.as_nanos(),
        );

        for layout in [
            DiffLayout::Unified,
            DiffLayout::Vertical,
            DiffLayout::Stacked,
        ] {
            for granularity in [DiffGranularity::Line, DiffGranularity::Character] {
                measure_frames(&prepared, hunks, layout, granularity);
            }
        }
    }
}

fn measure_frames(
    prepared: &PreparedDocument,
    hunks: usize,
    layout: DiffLayout,
    granularity: DiffGranularity,
) {
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).expect("measurement terminal");
    let mut interaction = Interaction {
        selected_file: 1,
        preferences: ViewPreferences {
            layout,
            granularity,
            context_lines: 3,
        },
        viewport: Viewport::new(120, 36),
    };
    let file = prepared.file(1).expect("second measurement file");
    let files = prepared.file_rows(1);
    terminal
        .draw(|frame| render_prepared_frame(frame, &files, file, &interaction, u16::MAX))
        .expect("initial measurement frame");

    for sample in 0..16 {
        let (key, input) = match sample % 4 {
            0 => ('l', Input::Right),
            1 => ('h', Input::Left),
            2 => ('j', Input::Down),
            _ => ('k', Input::Up),
        };
        let started = Instant::now();
        let bounds = navigation_bounds(prepared, &interaction, &input);
        let bounds_elapsed = started.elapsed();
        let Transition::RedrawInteraction(next) =
            transition_interaction(&interaction, input, &bounds)
        else {
            panic!("movement must retain interaction state");
        };
        interaction = next;
        let draw_started = Instant::now();
        let files = prepared.file_rows(interaction.selected_file);
        terminal
            .draw(|frame| render_prepared_frame(frame, &files, file, &interaction, u16::MAX))
            .expect("movement measurement frame");
        let frame_elapsed = draw_started.elapsed();
        let total_elapsed = started.elapsed();

        if sample >= 4 {
            println!(
                "movement hunks_per_file={hunks} layout={layout:?} detail={granularity:?} \
                 sample={} key={key} bounds_ns={} frame_ns={} input_frame_ns={}",
                sample - 4,
                bounds_elapsed.as_nanos(),
                frame_elapsed.as_nanos(),
                total_elapsed.as_nanos(),
            );
        }
    }
}

fn rust_patch(hunks: usize) -> String {
    let mut patch = String::new();
    let long_value = "abcdefghij".repeat(18);

    for file in 0..3 {
        patch.push_str(&format!("--- a/file-{file}.rs\n+++ b/file-{file}.rs\n"));
        for hunk in 0..hunks {
            let line = hunk * 7 + 1;
            patch.push_str(&format!(
                "@@ -{line},7 +{line},7 @@\n \
                 fn value_{hunk}() -> &'static str {{\n \
                     // A context row before the changed string.\n \
                     // Another context row before the changed string.\n\
                 -    \"before_{long_value}\"\n\
                 +    \"after_{long_value}\"\n \
                     // A context row after the changed string.\n \
                     // Another context row after the changed string.\n \
                 }}\n"
            ));
        }
    }

    patch
}
