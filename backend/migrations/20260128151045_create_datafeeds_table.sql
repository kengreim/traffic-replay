CREATE TABLE datafeeds
(
    id               BIGSERIAL PRIMARY KEY,
    update_timestamp TIMESTAMPTZ NOT NULL UNIQUE,
    pilots           JSONB       NOT NULL COMPRESSION lz4
);

CREATE INDEX idx_datafeeds_update_timestamp ON datafeeds (update_timestamp);
