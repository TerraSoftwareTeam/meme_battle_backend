-- Add max_players column to games table
ALTER TABLE games
ADD COLUMN max_players INT NOT NULL DEFAULT 8;
