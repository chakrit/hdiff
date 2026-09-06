# Git command output and diff-viewer compatibility

Sources: [Git pager configuration](https://git-scm.com/docs/git-config#Documentation/git-config.txt-pagerltcmdgt),
[show](https://git-scm.com/docs/git-show), [log](https://git-scm.com/docs/git-log),
[reflog](https://git-scm.com/docs/git-reflog), [stash](https://git-scm.com/docs/git-stash),
[range-diff](https://git-scm.com/docs/git-range-diff),
[format-patch](https://git-scm.com/docs/git-format-patch),
[diff](https://git-scm.com/docs/git-diff), [blame](https://git-scm.com/docs/git-blame),
and [status](https://git-scm.com/docs/git-status).

Provenance: official manuals consulted on 2026-09-06; output shapes independently
observed with Git 2.55.0 in a small isolated repository. This is an agent assessment
for maintainers evaluating Git input, not a new support contract or authorization
to register pagers. Finite-output results identify the candidate binary below;
interactive inspection found correct surrounding text and a source-display defect.

## Pager scope

`pager.<cmd>` applies to a Git subcommand when output goes to a terminal; it does not
select only patch-producing arguments. `--paginate` and `--no-pager` override that
configuration. This prevents treating a successful `git show <commit> | hdiff` check
as evidence that every `git show` invocation can use hdiff.
[Git configuration](https://git-scm.com/docs/git-config#Documentation/git-config.txt-pagerltcmdgt)

The following routing column names the relevant configuration surface for assessment;
it does not claim that registration was tested or recommend configuring it. Explicit
pipes avoid subcommand-wide registration. No additional registration is recommended
by this assessment; existing `pager.diff` installation remains governed by the spec.

## Observed output and preliminary recommendations

Every example below ran successfully as a Git producer. “Pipe candidate” means that
the captured output passed hdiff finite-output preservation. Interactive source fidelity
remains limited by the duplicated-token defect described below. Every deferred text-only
example is nonempty, so the successful zero-byte input rule does not apply.

| Invocation                                             | Observed shape and review value                                       | Routing surface                   | Preliminary recommendation                                                |
| ------------------------------------------------------ | --------------------------------------------------------------------- | --------------------------------- | ------------------------------------------------------------------------- |
| `git show HEAD~1`                                      | Commit metadata followed by one patch; review a commit                | `pager.show` or explicit pipe     | Pipe candidate; automatic registration outside scope                      |
| `git show HEAD` (empty commit)                         | Commit metadata without a patch                                       | `pager.show`                      | Deferred: nonempty text-only output                                       |
| `git show review` (annotated tag targeting a commit)   | Tag metadata, commit metadata, and target commit patch                | `pager.show` or explicit pipe     | Pipe candidate when target contributes a supported diff                   |
| `git show HEAD:`                                       | Tree label and entry names                                            | `pager.show`                      | Deferred: directory listing, no diff                                      |
| `git show HEAD:sample.rs`                              | Plain file contents                                                   | `pager.show`                      | Deferred: file display, no diff                                           |
| `git log -3`                                           | Commit metadata only                                                  | `pager.log`                       | Deferred: history listing, no diff                                        |
| `git log -p -3`                                        | Patchless entry followed by two commits changing the same path        | `pager.log` or explicit pipe      | Pipe candidate; keep both file occurrences and all commit text            |
| `git log --graph -p -3`                                | Graph columns prefix metadata, file headers, hunks, and records       | `pager.log`                       | Deferred: graph-prefixed patches need separate parsing support            |
| `git log -p -3 --format='REV %h%n%s'`                  | Custom commit labels followed by ordinary patches                     | `pager.log` or explicit pipe      | Pipe candidate for this tested format, not arbitrary formats              |
| `git log -p -1` (empty commit)                         | Commit metadata without a patch                                       | `pager.log`                       | Deferred: `-p` alone does not guarantee a diff                            |
| `git reflog -3`                                        | Reference updates with commit subjects; useful for recovery           | `pager.reflog` or explicit pipe   | Deferred: reflog listing, no diff                                         |
| `git stash show`                                       | Diffstat only under default configuration                             | Explicit pipe                     | Deferred: summary contains no file diff section                           |
| `git stash show -p`                                    | Ordinary patch of stashed changes relative to its base                | Explicit pipe                     | Pipe candidate; useful stash review                                       |
| `git range-diff HEAD~3..HEAD~2 HEAD~2..HEAD~1`         | Commit correspondence summary                                         | `pager.range-diff`                | Deferred: comparison of patch series, not unified file diffs              |
| `git range-diff --creation-factor=1000 …`              | Paired commit metadata and indented differences between patches       | `pager.range-diff`                | Deferred: nested patch representation needs a separate model              |
| `git format-patch --stdout HEAD~3..HEAD`               | Three mail messages, two patches, diffstats, and signature trailers   | Explicit pipe                     | Deferred: immediate signature delimiter is rejected as a hunk overrun     |
| `git blame HEAD -- sample.rs`                          | Source lines annotated with authorship                                | `pager.blame` or explicit pipe    | Deferred: annotated source, no diff                                       |
| `git status`                                           | Staged and unstaged path listings with instructions                   | `pager.status` or explicit pipe   | Deferred: repository status, no diff                                      |
| `git diff`                                             | Index-to-worktree patch                                               | Existing `pager.diff` or pipe     | Captured example preserves exact finite output                            |
| `git diff --cached`                                    | HEAD-to-index patch                                                   | Existing `pager.diff` or pipe     | Existing input shape; useful staged-change review                         |
| `git diff --stat`                                      | Diffstat only                                                         | Existing `pager.diff`             | Deferred output variant; bypass hdiff with `git --no-pager diff --stat`   |

## Reused Git constraints

`git show` dispatches by object type: commits include messages and patches, annotated
tags include their target, trees list names, and blobs emit contents. A tag targeting
a changed commit can therefore be a patch producer; “tag” alone does not determine
compatibility. Combined merge output remains outside the current hdiff scope.
[Git show](https://git-scm.com/docs/git-show)

`git log --graph` adds drawing characters at the left of output. The isolated sample
contains `| diff --git`, `| ---`, and `| @@`, so those lines are not ordinary unified
diff headers at column zero. The custom-format example leaves patch headers intact;
other custom formats still require their own verification, including text resembling
patch delimiters. [Git log](https://git-scm.com/docs/git-log)

`git reflog show` is a log-based reference history view. `git stash show` defaults to
a diffstat, with configuration able to change that default; explicit `-p` selects a
patch. Plain `log` and `stash show` therefore cannot inherit compatibility from their
patch-producing variants. [Git reflog](https://git-scm.com/docs/git-reflog),
[Git stash](https://git-scm.com/docs/git-stash)

`range-diff` compares patch series and has intentionally changeable human-readable
output. Its paired output includes indented `@@ Metadata` and nested change markers,
which are not ordinary file hunks. It requires a separate product and parser decision.
[Git range-diff](https://git-scm.com/docs/git-range-diff#_output_stability)

`format-patch --stdout` emits mail messages rather than filenames. The sample includes
the standalone `---` mail separator, diffstat, patch, and `-- ` signature delimiter;
all surrounding text must survive review. MIME attachments, transfer encoding,
cover-letter variations, and embedded comparison options were not exercised.
[Git format-patch](https://git-scm.com/docs/git-format-patch)

The captured default mail output is rejected at line 19: its `-- ` signature delimiter
immediately follows the completed hunk and starts like an extra deletion record.
hdiff reports `hunk records exceed declared counts`; strict hunk validation takes
precedence over treating that ambiguous line as trailing text. Default `format-patch`
output is therefore deferred; no mail-specific parser exception is part of this work.

`git diff --cached` changes the comparison endpoints without changing ordinary patch
syntax. `--stat` changes output to a summary; installing `pager.diff` does not make
that non-patch variant suitable for hdiff. [Git diff](https://git-scm.com/docs/git-diff)

## Local evidence and remaining verification

The retained runtime evidence is `.ace/git-compatibility/manifest.json`,
`.ace/git-compatibility/outputs/`, and `.ace/git-compatibility/repo/`. The manifest
records twenty commands, byte counts, and zero exit statuses; `range-diff-paired.txt`
adds the `--creation-factor=1000` variant using the same ranges as the table.
These paths are temporary local evidence, not committed prerequisites.

The fixture has an initial commit, two successive edits to `sample.rs`, an empty
commit at `HEAD`, and annotated tag `review` at `HEAD~1`. Its index and worktree have
distinct later edits. `stash create` and `stash store` retain a stash without clearing
the fixture worktree. Global/system Git configuration was excluded, hooks and commit
signing were disabled for fixture operations, and Git paging/color were disabled for
captures; the user's Git configuration was not changed.

Finite checks cover all 21 captured inputs. Seven patch-producing inputs returned
status 0 with byte-for-byte source preservation and empty stderr. Thirteen unsupported
inputs returned status 1 with `input contains no diff`; `format-patch` returned status 1
with `hunk records exceed declared counts`. The paired range-diff capture is counted
separately. Exact outcomes and working-tree provenance are retained in
`.ace/git-compatibility/hdiff-verification.json`.

The checked binary was `target/debug/hdiff`, SHA-256
`8cd9d0d7c8f4366c3ad1fcac05cd81bb873fa775bd4d3c4832847f69d0e708ca`,
from the working tree at `ee0897a040ddb56f1973fdf830d605062f3b7f6e` with the surrounding-text
implementation uncommitted. This identifies the measured artifact without claiming
that the commit alone reproduces it.

The actual `git show` pipe displayed surrounding commit text correctly in unified,
vertical, and stacked layouts, but Rust source rendering duplicated tokens:
`fn value() -> i32` appeared as `fnfn valuevalue() -> i32i32`. The finite-output checks
above preserve that same source correctly. The terminal captures include
`.ace/qol-show-vertical.txt` and `.ace/qol-show-stacked.txt`. This is evidence for the
separately lodged token-duplication task, not verified interactive source fidelity.
Recommendations remain limited to verified finite preservation and the inspected
surrounding-text display. Repeated-file navigation preserved two `sample.rs` entries
and their distinct messages, including leading patchless-commit text; captures are
`.ace/qol-log-first.txt` and `.ace/qol-log-second.txt`.

The actual `git show` producer can be exercised from the hdiff repository with:

```sh
GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 \
  git -C .ace/git-compatibility/repo -c color.ui=false --no-pager show HEAD~1 \
  | target/debug/hdiff
```

Its expected plain input is `.ace/git-compatibility/outputs/show.txt`.
