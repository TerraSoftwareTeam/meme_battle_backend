-- Set default is_ready to true in game_players table
ALTER TABLE game_players
ALTER COLUMN is_ready SET DEFAULT true;
