CREATE EXTENSION IF NOT EXISTS pgcrypto;

CREATE TYPE user_role AS ENUM (
    'user',
    'moderator',
    'admin'
);

CREATE TYPE media_status AS ENUM (
    'pending',
    'attached',
    'deleted'
);

CREATE TYPE media_visibility AS ENUM (
    'private',
    'public'
);
