-- 1. ENUMS
CREATE TYPE content_safety_level AS ENUM ('family_friendly', 'spicy', 'explicit');
CREATE TYPE game_status AS ENUM ('lobby', 'playing', 'finished');
CREATE TYPE round_phase AS ENUM ('waiting', 'submitting', 'voting', 'finished');
CREATE TYPE game_mode AS ENUM ('situation_to_meme', 'meme_to_situation');

-- 2. PACKS (SITUATIONS AND MEMES)
CREATE TABLE situation_packs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    author_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    language_code TEXT NOT NULL,
    safety_level content_safety_level NOT NULL DEFAULT 'family_friendly',
    is_public BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE pack_situations (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    pack_id UUID NOT NULL REFERENCES situation_packs(id) ON DELETE CASCADE,
    prompt_text TEXT NOT NULL,
    CONSTRAINT uq_pack_situations_pack_prompt UNIQUE (pack_id, prompt_text)
);

CREATE TABLE meme_packs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    author_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    description TEXT,
    language_code TEXT NOT NULL,
    safety_level content_safety_level NOT NULL DEFAULT 'family_friendly',
    is_public BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE pack_memes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    pack_id UUID NOT NULL REFERENCES meme_packs(id) ON DELETE CASCADE,
    media_id BIGINT REFERENCES media_assets(id) ON DELETE CASCADE,
    CONSTRAINT uq_pack_memes_pack_media UNIQUE (pack_id, media_id)
);

-- 3. GAME SESSIONS (AGGREGATES)
CREATE TABLE games (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    host_id UUID NOT NULL REFERENCES users(id),
    mode game_mode NOT NULL DEFAULT 'situation_to_meme',
    status game_status NOT NULL DEFAULT 'lobby',
    max_rounds INT NOT NULL DEFAULT 3,
    hand_size INT NOT NULL DEFAULT 5,
    current_round INT NOT NULL DEFAULT 0,
    version BIGINT NOT NULL DEFAULT 1,
    started_at TIMESTAMPTZ,
    finished_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE game_selected_situation_packs (
    game_id UUID NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    pack_id UUID NOT NULL REFERENCES situation_packs(id) ON DELETE CASCADE,
    PRIMARY KEY (game_id, pack_id)
);

CREATE TABLE game_selected_meme_packs (
    game_id UUID NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    pack_id UUID NOT NULL REFERENCES meme_packs(id) ON DELETE CASCADE,
    PRIMARY KEY (game_id, pack_id)
);

CREATE TABLE game_players (
    game_id UUID NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    score INT NOT NULL DEFAULT 0,
    is_ready BOOLEAN NOT NULL DEFAULT false,
    joined_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (game_id, user_id)
);

CREATE TABLE game_player_hand (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    game_id UUID NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    meme_id UUID REFERENCES pack_memes(id) ON DELETE CASCADE,
    situation_id UUID REFERENCES pack_situations(id) ON DELETE CASCADE,
    is_used BOOLEAN NOT NULL DEFAULT false,
    CHECK (num_nonnulls(meme_id, situation_id) = 1)
);

CREATE UNIQUE INDEX idx_game_player_hand_meme ON game_player_hand(game_id, user_id, meme_id) WHERE meme_id IS NOT NULL;
CREATE UNIQUE INDEX idx_game_player_hand_situation ON game_player_hand(game_id, user_id, situation_id) WHERE situation_id IS NOT NULL;

CREATE TABLE game_player_reserve (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    game_id UUID NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    draw_order INT NOT NULL,
    meme_id UUID REFERENCES pack_memes(id) ON DELETE CASCADE,
    situation_id UUID REFERENCES pack_situations(id) ON DELETE CASCADE,
    is_drawn BOOLEAN NOT NULL DEFAULT false,
    CHECK (num_nonnulls(meme_id, situation_id) = 1),
    UNIQUE (game_id, user_id, draw_order)
);

CREATE TABLE game_content_locks (
    game_id UUID NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    meme_id UUID REFERENCES pack_memes(id) ON DELETE CASCADE,
    situation_id UUID REFERENCES pack_situations(id) ON DELETE CASCADE,
    locked_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (num_nonnulls(meme_id, situation_id) = 1),
    UNIQUE (game_id, meme_id),
    UNIQUE (game_id, situation_id)
);

-- 4. ROUNDS AND SUBMISSIONS
CREATE TABLE game_rounds (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    game_id UUID NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    round_number INT NOT NULL,
    prompt_situation_id UUID REFERENCES pack_situations(id) ON DELETE CASCADE,
    prompt_meme_id UUID REFERENCES pack_memes(id) ON DELETE CASCADE,
    phase round_phase NOT NULL DEFAULT 'submitting',
    winner_user_id UUID REFERENCES users(id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (num_nonnulls(prompt_situation_id, prompt_meme_id) = 1)
);

CREATE TABLE round_submissions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    round_id UUID NOT NULL REFERENCES game_rounds(id) ON DELETE CASCADE,
    user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    submission_meme_id UUID REFERENCES pack_memes(id) ON DELETE CASCADE,
    submission_situation_id UUID REFERENCES pack_situations(id) ON DELETE CASCADE,
    submitted_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (round_id, user_id),
    CHECK (num_nonnulls(submission_meme_id, submission_situation_id) = 1)
);

CREATE TABLE round_votes (
    round_id UUID NOT NULL REFERENCES game_rounds(id) ON DELETE CASCADE,
    voter_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    submission_id UUID NOT NULL REFERENCES round_submissions(id) ON DELETE CASCADE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (round_id, voter_id)
);

-- 5. EVENT SOURCING AND OUTBOX
CREATE TABLE game_events (
    id UUID PRIMARY KEY,
    game_id UUID NOT NULL REFERENCES games(id) ON DELETE CASCADE,
    version BIGINT NOT NULL,
    type TEXT NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    -- Optimistic Concurrency Control: one event per version slot per game
    CONSTRAINT uq_game_events_game_version UNIQUE (game_id, version)
);

CREATE TABLE centrifugo_outbox (
    id BIGSERIAL PRIMARY KEY,
    method TEXT NOT NULL,
    payload JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

