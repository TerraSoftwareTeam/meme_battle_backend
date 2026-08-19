-- 1. REALTIME OUTBOX TABLE
CREATE TABLE realtime_outbox (
    event_id UUID PRIMARY KEY,
    game_id UUID NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    channel TEXT NOT NULL,
    payload JSONB NOT NULL,
    retry_count INT NOT NULL DEFAULT 0,
    next_retry_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Index for the recovery worker queries
CREATE INDEX idx_realtime_outbox_retry 
ON realtime_outbox (next_retry_at) 
WHERE retry_count <= 10;

-- 2. NOTIFY TRIGGER FOR REALTIME WORKER
CREATE OR REPLACE FUNCTION notify_realtime_outbox_inserted()
RETURNS TRIGGER AS $$
BEGIN
    PERFORM pg_notify('realtime_outbox_inserted', '');
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_realtime_outbox_inserted
AFTER INSERT ON realtime_outbox
FOR EACH STATEMENT
EXECUTE FUNCTION notify_realtime_outbox_inserted();
