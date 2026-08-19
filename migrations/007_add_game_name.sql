-- 007_add_game_name.sql
ALTER TABLE games ADD COLUMN name VARCHAR(100) NOT NULL DEFAULT 'Game';
