# docs

Durable artifacts. **File by the gate below** — walk it top to bottom and stop at the
first yes. The bottom (`scratch/`) charges a toll, so nothing lands there by default.

## Where does this go?

1. Third-party facts you keep to look up (a framework, an external API/CLI)? →
   [`vendor/`](vendor/) — link-first, mark provenance.
2. A how-to — using the product *or* operating the repo? → [`guides/`](guides/) — script
   repeatable operations; the guide holds the judgment.
3. How our system is built or meant to work, including its own config/CLI surface? →
   [`spec/`](spec/).
4. None of the above — genuinely unsettled exploration → [`scratch/`](scratch/). Open with
   a one-line "not spec because ___".

Everything settled amends `spec/`. The spec is authoritative; read it before working.

**Before writing into a folder, read that folder's `README.md` first.**
