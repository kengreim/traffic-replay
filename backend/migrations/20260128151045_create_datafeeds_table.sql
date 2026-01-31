CREATE TABLE datafeeds
(
    id               BIGSERIAL PRIMARY KEY,
    update_timestamp TIMESTAMPTZ NOT NULL UNIQUE,
    pilots           TEXT        COMPRESSION lz4 NOT NULL
);

CREATE INDEX idx_datafeeds_update_timestamp ON datafeeds (update_timestamp);
