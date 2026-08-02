# Scheduler Plan: Automatic VATUSA Event Collation

## New binary: `scheduler`

A long-running process that polls the VATSIM events API, tracks event lifecycle, and triggers collation + upload automatically.

## Database

Add an `events` table to track discovered events and their processing state:

```sql
CREATE TABLE events (
    id          BIGINT PRIMARY KEY,  -- VATSIM event ID
    name        TEXT NOT NULL,
    airports    TEXT[] NOT NULL,
    start_time  TIMESTAMPTZ NOT NULL,
    end_time    TIMESTAMPTZ NOT NULL,
    status      TEXT NOT NULL DEFAULT 'pending'
                -- pending → collating → completed / failed / skipped
);
```

## Event discovery

- Poll `https://my.vatsim.net/api/v2/events/view/division/USA` periodically (~30 min)
- Insert new events into the `events` table (ignore duplicates by ID)
- Store all events in the database for record-keeping
- Only trigger collation for events that have at least one airport

## ARTCC lookup

- Add `RESP_ARTCC_ID` to `AirportRecord` and carry it through to the `Airport` struct
- When building `EventConfig` from the API, derive `artccs` by looking up each airport's `RESP_ARTCC_ID` from the CSV and deduplicating

## Collation trigger

- After an event's `end_time` + post-buffer has passed, and datafeeds exist for the window, set status to `collating` and run the collation logic
- Extract the core collation logic from `collator.rs` into a library function callable by both the scheduler and the existing CLI binary:
  ```rust
  pub async fn collate_event(
      pool: &PgPool,
      airports_db: &HashMap<String, Airport>,
      event: &EventConfig,
  ) -> Result<Vec<u8>, anyhow::Error>
  ```

## Upload + events.json

- Upload the gzipped event data to R2 (same as today)
- Update `events.json` in R2:
  1. Download current `events.json` from R2
  2. Append the new event entry
  3. Upload the updated `events.json` back to R2
- Set event status to `completed`

## Error handling

- If collation or upload fails, set status to `failed` and log the error
- No automatic retries — failed events stay as `failed` and can be investigated/retried manually

## Manual event editing

Edit pending events directly via SQL against the Postgres instance. The scheduler only processes events with status `pending`, so edits are safe any time before collation starts.

```sql
-- Adjust airports
UPDATE events SET airports = '{KJFK,KLGA,KEWR}' WHERE id = 12345;

-- Adjust times
UPDATE events SET start_time = '2026-05-20T22:00:00Z', end_time = '2026-05-21T03:00:00Z' WHERE id = 12345;

-- Skip an event
UPDATE events SET status = 'skipped' WHERE id = 12345;
```

The scheduler ignores events with status `skipped`, `completed`, or `failed` — only `pending` events are eligible for collation.

## Dockerfile

Update the existing Dockerfile to build both binaries and include `APT_BASE.csv`:

```dockerfile
RUN cargo build --release --bin fetcher --bin scheduler

COPY --from=builder /app/target/release/fetcher /usr/local/bin/fetcher
COPY --from=builder /app/target/release/scheduler /usr/local/bin/scheduler
COPY APT_BASE.csv /usr/local/bin/APT_BASE.csv
```

- Default CMD remains `fetcher` so the existing deployment is unaffected
- Run the scheduler as a separate container from the same image, overriding the command to `scheduler`
- The scheduler container needs the same `DATABASE_URL` and network as the fetcher, plus `R2_ENDPOINT_URL`, `R2_BUCKET`, and AWS credentials
