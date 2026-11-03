CREATE TABLE events
(
    id              BIGSERIAL PRIMARY KEY,
    vatusa_id       BIGINT UNIQUE,
    vatsim_id       BIGINT UNIQUE,
    -- Latest raw API payloads and the config proposed from them
    vatusa_source   JSONB,
    vatsim_source   JSONB,
    proposal        JSONB,
    -- Editable config (maps onto EventConfig)
    name            TEXT        NOT NULL,
    start_time      TIMESTAMPTZ NOT NULL,
    end_time        TIMESTAMPTZ NOT NULL,
    airports        TEXT[]      NOT NULL DEFAULT '{}',
    artccs          TEXT[]      NOT NULL DEFAULT '{}',
    custom_airports JSONB       NOT NULL DEFAULT '{}',
    airport_source  TEXT        NOT NULL DEFAULT 'none',
    unlisted        BOOLEAN     NOT NULL DEFAULT FALSE,
    edited          BOOLEAN     NOT NULL DEFAULT FALSE,
    status          TEXT        NOT NULL DEFAULT 'proposed'
        CHECK (status IN ('proposed', 'approved', 'collating', 'published', 'failed', 'skipped')),
    error           TEXT,
    published_url   TEXT,
    approved_at     TIMESTAMPTZ,
    collated_at     TIMESTAMPTZ,
    last_seen_at    TIMESTAMPTZ,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_events_status_end_time ON events (status, end_time);
CREATE INDEX idx_events_start_time ON events (start_time);
