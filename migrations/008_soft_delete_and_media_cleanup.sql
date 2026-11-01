-- 1. Add deleted_at to meme_packs and situation_packs
ALTER TABLE meme_packs ADD COLUMN deleted_at TIMESTAMPTZ DEFAULT NULL;
CREATE INDEX idx_meme_packs_active ON meme_packs (id) WHERE deleted_at IS NULL;

ALTER TABLE situation_packs ADD COLUMN deleted_at TIMESTAMPTZ DEFAULT NULL;
CREATE INDEX idx_situation_packs_active ON situation_packs (id) WHERE deleted_at IS NULL;

-- 2. Migrate pack_memes from is_active to deleted_at, update FK to ON DELETE SET NULL
ALTER TABLE pack_memes ADD COLUMN deleted_at TIMESTAMPTZ DEFAULT NULL;
UPDATE pack_memes SET deleted_at = now() WHERE is_active = false;
DROP INDEX IF EXISTS idx_pack_memes_pack_active;
ALTER TABLE pack_memes DROP COLUMN is_active;
CREATE INDEX idx_pack_memes_active ON pack_memes (pack_id) WHERE deleted_at IS NULL;

ALTER TABLE pack_memes DROP CONSTRAINT IF EXISTS pack_memes_media_id_fkey;
ALTER TABLE pack_memes ADD CONSTRAINT pack_memes_media_id_fkey 
    FOREIGN KEY (media_id) REFERENCES media_assets(id) ON DELETE SET NULL;

-- 3. Migrate pack_situations from is_active to deleted_at
ALTER TABLE pack_situations ADD COLUMN deleted_at TIMESTAMPTZ DEFAULT NULL;
UPDATE pack_situations SET deleted_at = now() WHERE is_active = false;
DROP INDEX IF EXISTS idx_pack_situations_pack_active;
ALTER TABLE pack_situations DROP COLUMN is_active;
CREATE INDEX idx_pack_situations_active ON pack_situations (pack_id) WHERE deleted_at IS NULL;

-- 4. Queue for asynchronous background cleanup of CDN media files
CREATE TABLE media_purge_tasks (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    media_asset_id BIGINT NOT NULL,
    provider TEXT NOT NULL,
    provider_file_id TEXT NOT NULL,
    retry_count INT NOT NULL DEFAULT 0,
    next_retry_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_error TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_media_purge_tasks_pickup ON media_purge_tasks (next_retry_at);
