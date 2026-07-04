-- Create realtime_outbox table
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

-- Drop the old outbox table
DROP TABLE centrifugo_outbox;
