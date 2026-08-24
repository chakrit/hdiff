# Testing the terminal UI

Run the terminal smoke suite from the repository root:

```sh
nice -n 19 cargo build --quiet
smoke tests/terminal-ui/smoke.yml
```

The suite opens `hdiff` in a 120-by-36 tmux session, captures the ANSI-styled first frame and
each Tab-selected fixture file, then sends `q`. The script owns a uniquely named session and
removes it through an exit trap, including after a timeout or capture failure.

`UNCHANGED` means the captured terminal surface matches the committed lock; it does not establish
visual correctness. For an intended capture change, review `smoke --print` output and run
`smoke --commit tests/terminal-ui/smoke.yml` only after accepting the new terminal surface.
