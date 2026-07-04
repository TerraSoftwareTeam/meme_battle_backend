-- Create trigger function to notify on outbox inserts
CREATE OR REPLACE FUNCTION notify_realtime_outbox_inserted()
RETURNS TRIGGER AS $$
BEGIN
    PERFORM pg_notify('realtime_outbox_inserted', '');
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Attach trigger to realtime_outbox table
CREATE TRIGGER trigger_realtime_outbox_inserted
AFTER INSERT ON realtime_outbox
FOR EACH STATEMENT
EXECUTE FUNCTION notify_realtime_outbox_inserted();
