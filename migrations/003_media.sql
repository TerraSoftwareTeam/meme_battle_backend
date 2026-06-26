CREATE TABLE media_assets (
    id                   BIGSERIAL PRIMARY KEY,
    owner_user_id        UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider             TEXT NOT NULL,
    provider_file_id     TEXT NOT NULL,
    url                  TEXT NOT NULL,
    filename             TEXT NOT NULL,
    content_type         TEXT NOT NULL,
    size_bytes           BIGINT NOT NULL CHECK (size_bytes >= 0),
    status               media_status NOT NULL DEFAULT 'pending',
    visibility           media_visibility NOT NULL DEFAULT 'private',
    created_at           TIMESTAMPTZ NOT NULL DEFAULT now(),
    modified_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (provider, provider_file_id)
);

CREATE INDEX idx_media_assets_owner_created
    ON media_assets(owner_user_id, created_at DESC);

CREATE INDEX idx_media_assets_status_created
    ON media_assets(status, created_at);

ALTER TABLE users
    ADD CONSTRAINT fk_users_avatar_media
    FOREIGN KEY (avatar_media_asset_id)
    REFERENCES media_assets(id)
    ON DELETE SET NULL;
