-- Split long events into fixed-length parts, each collated into its own file
ALTER TABLE events ADD COLUMN part_hours INTEGER CHECK (part_hours > 0);

-- Collation attempts since the event was last approved, so an event that keeps
-- interrupting collation (e.g. by exhausting memory) isn't retried forever
ALTER TABLE events ADD COLUMN attempts INTEGER NOT NULL DEFAULT 0;
