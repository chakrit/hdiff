    /// docs/spec/architecture.md#input-contract: structural parsing is strict, so a
    /// header count is ASCII digits and nothing else.
    #[test]
    fn rejects_signed_header_counts() {
        let files = "--- a/file\n+++ b/file\n";
        for header in ["@@ -+1,+1 +1 @@", "@@ -1,+1 +1 @@", "@@ -1 ++1 @@"] {
            let input = format!("{files}{header}\n-old\n+new\n");

            assert_eq!(
                parse_patch(input.as_bytes()),
                malformed(3, Malformed::InvalidHunkHeader),
                "{header}"
            );
        }
    }

    /// docs/spec/architecture.md#input-contract: every line inside a hunk is a marked
    /// record, a blank context line, or a no-newline marker.
    #[test]
    fn rejects_unmarked_lines_inside_hunks() {
        let files = "--- a/x\n+++ b/x\n";
        for (hunk, line) in [
            ("@@ -1 +1 @@\nhello\n-a\n+b\n", 4),
            ("@@ -2 +2 @@\n-a\nhello world\n+b\n", 5),
        ] {
            let input = format!("{files}{hunk}");

            assert_eq!(
                parse_patch(input.as_bytes()),
                malformed(line, Malformed::UnmarkedHunkLine),
                "{hunk}"
            );
        }
    }

    /// docs/spec/architecture.md#input-contract: GNU `diff --suppress-blank-empty`
    /// writes a blank context line as an empty line.
    #[test]
    fn reads_an_empty_line_inside_a_hunk_as_blank_context() {
        let input = b"--- a/x\n+++ b/x\n@@ -1,3 +1,3 @@\n a\n\n-b\n+c\n";

        let document = parse_patch(input).expect("GNU blank context");
        let kinds = first_hunk(&document)
            .records
            .iter()
            .map(|record| record.kind)
            .collect::<Vec<_>>();

        assert_eq!(
            kinds,
            [
                RecordKind::Context(ContextMarker::Space),
                RecordKind::Context(ContextMarker::Blank),
                RecordKind::Deletion,
                RecordKind::Addition,
            ]
        );
        assert_eq!(finite_output::of_document(&document), input);
    }

    /// docs/spec/architecture.md#input-contract: the marker is known by its `\ ` prefix,
    /// since producers translate the rest.
    #[test]
    fn keeps_a_translated_no_newline_marker_with_its_hunk() {
        let input = "--- a/x\n+++ b/x\n@@ -1 +1 @@\n-a\n+b\n\\ Kein Zeilenumbruch am Dateiende.\n--- a/y\n+++ b/y\n@@ -1 +1 @@\n-c\n+d\n";

        let document = parse_patch(input.as_bytes()).expect("translated marker");
        let views = document.file_views().collect::<Vec<_>>();
        let [first, second] = views.as_slice() else {
            panic!("the patch holds two files: {} views", views.len());
        };
        let FileBody::Unified(first) = &first.file.body else {
            panic!("first file is unified");
        };
        let marker = first
            .hunks
            .first()
            .records
            .last()
            .and_then(|record| record.no_newline.as_ref())
            .map(|marker| marker.sanitized());

        assert_eq!(marker, Some("\\ Kein Zeilenumbruch am Dateiende."));
        assert!(
            second.leading.is_empty(),
            "the marker is not the next file's text: {:?}",
            second.leading
        );
    }

    /// docs/spec/architecture.md#input-contract: a no-newline marker follows a record.
    #[test]
    fn rejects_a_no_newline_marker_that_follows_no_record() {
        let files = "--- a/x\n+++ b/x\n";
        let marker = "\\ No newline at end of file\n";
        for (hunk, line) in [
            (format!("@@ -1 +1 @@\n{marker}-a\n+b\n"), 4),
            (format!("@@ -1 +1 @@\n-a\n{marker}{marker}+b\n"), 6),
            (format!("@@ -1 +1 @@\n-a\n+b\n{marker}{marker}"), 7),
        ] {
            let input = format!("{files}{hunk}");

            assert_eq!(
                parse_patch(input.as_bytes()),
                malformed(line, Malformed::MisplacedNoNewlineMarker),
                "{hunk}"
            );
        }
    }

    /// docs/spec/architecture.md#input-contract: a no-newline marker ends its record's
    /// side of the file, so no later record on that side follows it, in its hunk or a
    /// later one.
    #[test]
    fn rejects_a_record_after_the_marker_that_ended_its_side() {
        let files = "--- a/f\n+++ b/f\n";
        let marker = "\\ No newline at end of file\n";
        for (hunks, line) in [
            (format!("@@ -1,2 +1 @@\n-a\n{marker}-b\n+c\n"), 6),
            (
                format!("@@ -1 +1 @@\n-a\n{marker}+c\n@@ -5 +5 @@\n-x\n+y\n"),
                8,
            ),
            (format!("@@ -1,2 +0,0 @@\n-a\n{marker}-b\n{marker}"), 6),
            (format!("@@ -1,2 +1,2 @@\n a\n{marker}-b\n+c\n"), 6),
        ] {
            let input = format!("{files}{hunks}");

            assert_eq!(
                parse_patch(input.as_bytes()),
                malformed(line, Malformed::RecordAfterNoNewlineMarker),
                "{hunks}"
            );
        }

        let other_side = format!("{files}@@ -1 +1 @@\n-a\n{marker}+c\n{marker}");
        assert!(
            parse_patch(other_side.as_bytes()).is_ok(),
            "an addition follows the old side's marker"
        );
    }

    /// docs/spec/architecture.md#input-contract: a hunk header is
    /// `@@ -<range> +<range> @@`, single-spaced as producers write it and `git apply`
    /// reads it.
    #[test]
    fn rejects_hunk_headers_spaced_other_than_producers_write() {
        let files = "--- a/x\n+++ b/x\n";
        for header in [
            "@@-1 +1 @@",
            "@@  -1  +1  @@",
            "@@ -1\t+1 @@",
            "@@ -1 +1  @@",
            "@@ -1  +1 @@",
        ] {
            let input = format!("{files}{header}\n-a\n+b\n");

            assert_eq!(
                parse_patch(input.as_bytes()),
                malformed(3, Malformed::InvalidHunkHeader),
                "{header:?}"
            );
        }

        let headed = format!("{files}@@ -1 +1 @@ fn main() {{\n-a\n+b\n");
        assert!(
            parse_patch(headed.as_bytes()).is_ok(),
            "a section heading follows"
        );
    }

    #[test]
    fn rejects_diff_headers_inside_incomplete_hunks() {
        let files = "--- a/file\n+++ b/file\n@@ -1 +1 @@\n";
        for (header, reason) in [
            ("@@ malformed", Malformed::UnexpectedDiffHeader),
            ("diff --git a/g b/g", Malformed::UnexpectedGitHeaderInHunk),
        ] {
            let input = format!("{files}{header}\n-old\n+new\n");

            assert_eq!(
                parse_patch(input.as_bytes()),
                malformed(4, reason),
                "{header}"
            );
        }
    }

    #[test]
    fn rejects_records_exceeding_hunk_counts() {
        let valid = "--- a/file\n+++ b/file\n@@ -1 +1 @@\n-old\n+new\n";
        for suffix in ["-extra\n", "+extra\n", " context\n"] {
            let input = format!("{valid}{suffix}");

            assert_eq!(
                parse_patch(input.as_bytes()),
                malformed(6, Malformed::ExcessRecords),
                "{suffix}"
            );
        }
    }

    #[test]
    fn rejects_the_record_that_overshoots_one_side_of_a_hunk() {
        let overfull = "--- a/f\n+++ b/f\n@@ -1 +1 @@\n-a\n-b\n+c\n";
        for suffix in ["", "--- a/g\n+++ b/g\n@@ -1 +1 @@\n-x\n+y\n"] {
            let input = format!("{overfull}{suffix}");

            assert_eq!(
                parse_patch(input.as_bytes()),
                malformed(5, Malformed::ExcessRecords),
                "{suffix}"
            );
        }
    }

