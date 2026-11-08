# Scheduler Plan: Event Discovery, Review and Collation

## Overview

A new long-running binary, `scheduler`, that:

1. Polls the VATUSA and VATSIM event APIs and builds a proposed config (name, times, airports, ARTCCs) for each upcoming event
2. Serves a small web UI where events are reviewed, edited and approved
3. Collates and publishes **only approved events**, once the event has ended plus the post-event buffer

Nothing is collated or uploaded without explicit approval. The existing `collator` CLI keeps working from `config.toml`.

## Event sources

### VATUSA (primary list of events)

- `GET https://api.vatusa.net/v3/events?start_date_from=<7 days ago>&count=100`, paginated with `X-Total-Pages`
- Anonymous requests only return approved events, which is what we want
- Fields used: `id`, `title`, `body`, `facility`, `start_time`, `end_time` (unix seconds)
- `facility` is the host ARTCC (`ZHQ` for division-hosted events, which contributes no ARTCC)

### VATSIM (airports, when available)

- `GET https://my.vatsim.net/api/v2/events/view/division/USA`
- Fields used: `id`, `name`, `start_time`, `end_time`, `airports[].icao`
- Match to a VATUSA event when the time windows overlap (with 2h of slack) and the titles are similar (word overlap of at least 50%). On a match, VATSIM's airports are the proposal
- `organisers[].subdivision` is always null in practice, so it can't be used for matching
- VATSIM's airport list sometimes contains facility codes (`ZBW`, `ZUL`): US ARTCCs are moved to `artccs`, other `Z??` codes are dropped
- VATSIM events with no VATUSA match are still listed, as their own proposals

## Proposed config

| Field | Proposal |
|---|---|
| `name` | VATUSA `title` (VATSIM `name` if VATSIM-only) |
| `advertised_start_time` / `advertised_end_time` | VATUSA's, unless its end isn't after its start; then VATSIM's |
| `airports` | VATSIM airports if matched; otherwise codes extracted from title/body (below) |
| `artccs` | VATUSA `facility` (unless `ZHQ`) plus `RESP_ARTCC_ID` of each airport, deduplicated |

Airport extraction from text (HTML stripped first), validated against `APT_BASE.csv`:

- 4-letter ICAO codes (`KSEA`) that exist in the airport data and are towered or in the host ARTCC
- 3-letter FAA codes (`SEA`) only when the airport is in the host ARTCC and towered (`TWR_TYPE_CODE`), to avoid matching words like `FNO`, `ATC` or `THE`

This requires adding `RESP_ARTCC_ID` and `TWR_TYPE_CODE` to `AirportRecord` / `Airport`.

Proposals are a starting point only; anything can be corrected in the UI before approval.

## Database

See `migrations/20261103000000_create_events_table.sql`. Besides the editable config (which maps onto `EventConfig`), each row keeps the latest raw payload from each API, the latest `proposal`, where its airports came from (`vatsim`, `text`, `manual` or `none`), and when the APIs last listed it.

Both source IDs are nullable so an event can come from either API or be added by hand.

### Status lifecycle

```
proposed ──approve──▶ approved ──(end + buffer passed)──▶ collating ──▶ published
    │                    │                                    │
    └──skip──▶ skipped   └──unapprove──▶ proposed             └──▶ failed ──retry──▶ collating
```

- `proposed`: discovered, not yet reviewed. Never collated
- `approved`: will collate automatically once `end_time + EVENT_POST_TIME_MINUTES` has passed
- `published` and `failed` events can be edited and re-collated, which overwrites the R2 object and `events.json` entry

### Re-polling

- Re-polling refreshes `*_source` on every event
- It only rewrites the editable fields while the event is `proposed` **and** `edited = false`; any manual edit or approval stops further overwrites ("Reset to proposal" undoes this)
- A VATSIM-only row is merged into its VATUSA event once that appears, if the VATSIM row hasn't been touched
- If the source times change after approval, the UI shows a "source changed" warning rather than changing the config silently

## Collation trigger

A loop (every ~1 min) picks up events where `status = 'approved'` and `end_time + buffer <= NOW()`, sets them to `collating` and runs the collation. As a result:

- **Approved before the event**: collates automatically after it ends
- **Approved after the event ends**: picked up on the next tick, so approving is enough to trigger collation
- **Collate now** button: for re-collating `published`/`failed` events

Raw datafeeds are deleted after 7 days (`fetcher.rs`), so:

- The UI shows how long until each past event's data expires
- Collation is refused once `start_time - EVENT_PRE_TIME_MINUTES` is older than the retention window

If the scheduler restarts mid-collation, the event is requeued on startup. Each claim increments `attempts` (reset on approval or retry), and after 2 interrupted attempts the event is marked `failed` instead, so an event that keeps exhausting memory isn't retried forever.

### Parts

Long events can be split into fixed-length parts (`part_hours`), counted from the advertised start; the last part may be shorter. Each part is collated, uploaded and freed in turn, so only one part's capture is in memory at a time. The pre/post buffers apply only to the outer edges.

- Files: `<slug>-part-1.json`, `<slug>-part-2.json`, …; an unsplit event keeps `<slug>.json`
- `events.json`: `url` is part 1, and `parts: [{ start, end, url }]` lists every part (omitted when there is only one)
- Splitting or unsplitting a published event changes its URLs; the old files stay in R2

## Refactor collator

Extract the core of `collator.rs` into `lib.rs`:

```rust
pub async fn collate_event(
    pool: &PgPool,
    airports_db: &HashMap<String, Airport>,
    event: &EventConfig,
) -> Result<Vec<u8>, anyhow::Error> // gzipped JSON
```

along with `upload_to_r2`. The `collator` CLI becomes a thin wrapper that reads `config.toml`, calls these, and prints the `events.json` entry as it does today.

## Publishing to `events.json`

The frontend currently loads `events.json` from the GitHub repo (`src/consts.ts`), and it is updated by hand-made commits. For the scheduler to publish:

- Move `events.json` to R2 next to the event files, and point `EVENTS_METADATA_URL` at it (`https://data.vatsim-replay.com/events.json`)
- On publish: download `events.json`, insert or replace the entry for this event (matched by URL), sort by start time, upload
- One-time migration: copy the current `public/events.json` to R2 **before** deploying the frontend change. Publishing refuses to run if `events.json` is missing from R2, rather than creating a near-empty one

## Web UI

Served by the scheduler with `axum`, as server-rendered HTML forms (no separate frontend build).

- **List page**: upcoming and recent events grouped by status, with start time, airports and data expiry for past events
- **Event page**: edit name, times, airports, ARTCCs, custom airport coordinates and unlisted; buttons to approve, unapprove, skip and collate now; shows the raw VATUSA/VATSIM data side by side, plus `error` for failed events
- **Add event**: create an event by hand for anything neither API lists

Auth: HTTP basic auth checked against `ADMIN_USERNAME` / `ADMIN_PASSWORD`, plus a same-origin check on form posts (browsers resend basic auth credentials to cross-site requests). Compose binds the UI to `127.0.0.1:8080`; expose it over HTTPS (reverse proxy) or a private network.

## Configuration

New environment variables for the scheduler container:

- `VATUSA_API_URL` (default `https://api.vatusa.net/v3`), `VATSIM_EVENTS_URL` (default the USA division feed)
- `ADMIN_USERNAME` (default `admin`), `ADMIN_PASSWORD` (required)
- `SCHEDULER_BIND_ADDR` (default `0.0.0.0:8080`)
- Existing: `DATABASE_URL`, `R2_ENDPOINT_URL`, `R2_BUCKET`, AWS credentials

## Docker

Build both binaries and include `APT_BASE.csv`:

```dockerfile
RUN cargo build --release --bin fetcher --bin scheduler

COPY --from=builder /app/target/release/fetcher /usr/local/bin/fetcher
COPY --from=builder /app/target/release/scheduler /usr/local/bin/scheduler
COPY APT_BASE.csv /usr/local/bin/APT_BASE.csv
```

- Default CMD remains `fetcher`, so the existing deployment is unaffected
- Add a `scheduler` service to `compose.yaml` from the same image with `command: scheduler`, the same `DATABASE_URL`, the R2 variables and an exposed port for the UI

## Not in scope (for now)

- Checking airports against actual traffic after the event
- Confidence scoring or automatic approval
