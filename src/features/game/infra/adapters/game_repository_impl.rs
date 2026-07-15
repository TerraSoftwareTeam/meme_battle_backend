use async_trait::async_trait;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;
use chrono::{DateTime, Utc};


use crate::{
    common::http::error::AppError,
    features::game::domain::{
        ports::GameRepository,
        model::{
            Game, ActiveGame, RawGameCard, GameMode, GamePlayer, GamePlayerHandCard, GameRound, GameStatus,
            PlayerSubmissionState, RoundPhase, RoundSubmission, ContentSafetyLevel, LanguageCode,
            MemePack, PackMeme, SituationPack, PackSituation, GamePlayerHandCardWithMedia,
        },
    },
};

#[derive(Clone)]
pub struct GameRepositoryImpl {
    pool: PgPool,
}

impl GameRepositoryImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl GameRepository for GameRepositoryImpl {
    async fn find_game(&self, game_id: Uuid) -> Result<Option<Game>, AppError> {
        let game = sqlx::query_as::<_, Game>(
            r#"
            SELECT id, host_id, mode, status, max_rounds, hand_size, submit_time_limit, vote_time_limit, current_round, version, started_at, finished_at, created_at
            FROM games
            WHERE id = $1
            "#,
        )
        .bind(game_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(game)
    }

    async fn find_active_lobby_games(&self) -> Result<Vec<ActiveGame>, AppError> {
        let games = sqlx::query_as::<_, ActiveGame>(
            r#"
            SELECT g.id, g.host_id, g.mode, g.max_rounds, g.hand_size, g.created_at,
                   COUNT(gp.user_id)::int as players_count
            FROM games g
            LEFT JOIN game_players gp ON g.id = gp.game_id
            WHERE g.status = 'lobby'
            GROUP BY g.id
            ORDER BY g.created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(games)
    }

    async fn get_players(&self, game_id: Uuid) -> Result<Vec<GamePlayer>, AppError> {
        let players = sqlx::query_as::<_, GamePlayer>(
            r#"
            SELECT game_id, user_id, score, is_ready, handle, joined_at
            FROM game_players
            WHERE game_id = $1
            ORDER BY joined_at ASC
            "#,
        )
        .bind(game_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(players)
    }

    async fn get_player_hand(
        &self,
        game_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<RawGameCard>, AppError> {
        #[derive(sqlx::FromRow)]
        struct RawHandRow {
            meme_id: Option<Uuid>,
            situation_id: Option<Uuid>,
            media_id: Option<i64>,
            prompt_text: Option<String>,
        }

        let rows = sqlx::query_as::<_, RawHandRow>(
            r#"
            SELECT
                gph.meme_id,
                gph.situation_id,
                pm.media_id,
                ps.prompt_text
            FROM game_player_hand gph
            LEFT JOIN pack_memes pm ON gph.meme_id = pm.id
            LEFT JOIN pack_situations ps ON gph.situation_id = ps.id
            WHERE gph.game_id = $1 AND gph.user_id = $2 AND gph.is_used = false
            "#,
        )
        .bind(game_id)
        .bind(user_id)
        .fetch_all(&self.pool)
        .await?;

        let cards = rows
            .into_iter()
            .filter_map(|row| {
                if let Some(meme_id) = row.meme_id {
                    Some(RawGameCard::Meme {
                        id: meme_id,
                        media_id: row.media_id,
                    })
                } else if let Some(situation_id) = row.situation_id {
                    Some(RawGameCard::Situation {
                        id: situation_id,
                        prompt_text: row.prompt_text.unwrap_or_default(),
                    })
                } else {
                    None
                }
            })
            .collect();

        Ok(cards)
    }

    async fn get_player_hand_with_media(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<GamePlayerHandCardWithMedia>, AppError> {
        #[derive(sqlx::FromRow)]
        struct RawRow {
            meme_id: Option<Uuid>,
            situation_id: Option<Uuid>,
            media_id: Option<i64>,
            text: Option<String>,
        }

        let rows = sqlx::query_as::<_, RawRow>(
            r#"
            SELECT
                gph.meme_id,
                gph.situation_id,
                pm.media_id,
                ps.prompt_text AS text
            FROM game_player_hand gph
            LEFT JOIN pack_memes pm ON gph.meme_id = pm.id
            LEFT JOIN pack_situations ps ON gph.situation_id = ps.id
            WHERE gph.game_id = $1 AND gph.user_id = $2 AND gph.is_used = false
            "#,
        )
        .bind(game_id)
        .bind(user_id)
        .fetch_all(&mut **tx)
        .await?;

        let cards = rows
            .into_iter()
            .map(|row| {
                if let Some(meme_id) = row.meme_id {
                    GamePlayerHandCardWithMedia {
                        id: meme_id,
                        kind: "meme".to_string(),
                        media_id: row.media_id,
                        text: None,
                    }
                } else {
                    GamePlayerHandCardWithMedia {
                        id: row.situation_id.unwrap(),
                        kind: "situation".to_string(),
                        media_id: None,
                        text: row.text,
                    }
                }
            })
            .collect();

        Ok(cards)
    }

    async fn get_current_round(&self, game_id: Uuid) -> Result<Option<GameRound>, AppError> {
        let round = sqlx::query_as::<_, GameRound>(
            r#"
            SELECT gr.id, gr.game_id, gr.round_number, gr.prompt_situation_id, gr.prompt_meme_id, gr.phase, gr.winner_user_id, gr.phase_expires_at, gr.claimed_at, gr.claimed_by, gr.created_at
            FROM game_rounds gr
            JOIN games g ON gr.game_id = g.id
            WHERE gr.game_id = $1 AND gr.round_number = g.current_round
            "#,
        )
        .bind(game_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(round)
    }

    async fn get_round(&self, round_id: Uuid) -> Result<Option<GameRound>, AppError> {
        let round = sqlx::query_as::<_, GameRound>(
            r#"
            SELECT id, game_id, round_number, prompt_situation_id, prompt_meme_id, phase, winner_user_id, phase_expires_at, claimed_at, claimed_by, created_at
            FROM game_rounds
            WHERE id = $1
            "#,
        )
        .bind(round_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(round)
    }

    async fn get_prompt_card(
        &self,
        situation_id: Option<Uuid>,
        meme_id: Option<Uuid>,
    ) -> Result<Option<RawGameCard>, AppError> {
        if let Some(sit_id) = situation_id {
            let text = sqlx::query_scalar::<_, String>(
                r#"
                SELECT prompt_text
                FROM pack_situations
                WHERE id = $1
                "#,
            )
            .bind(sit_id)
            .fetch_optional(&self.pool)
            .await?;

            if let Some(text) = text {
                return Ok(Some(RawGameCard::Situation {
                    id: sit_id,
                    prompt_text: text,
                }));
            }
        } else if let Some(m_id) = meme_id {
            let media_id = sqlx::query_scalar::<_, Option<i64>>(
                r#"
                SELECT media_id
                FROM pack_memes
                WHERE id = $1
                "#,
            )
            .bind(m_id)
            .fetch_optional(&self.pool)
            .await?
            .flatten();

            return Ok(Some(RawGameCard::Meme {
                id: m_id,
                media_id,
            }));
        }
        Ok(None)
    }

    async fn get_available_memes(&self, game_id: Uuid) -> Result<Vec<Uuid>, AppError> {
        let ids = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT pm.id
            FROM pack_memes pm
            JOIN game_selected_meme_packs gsmp ON pm.pack_id = gsmp.pack_id
            WHERE gsmp.game_id = $1
            "#,
        )
        .bind(game_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(ids)
    }

    async fn get_available_situations(&self, game_id: Uuid) -> Result<Vec<Uuid>, AppError> {
        let ids = sqlx::query_scalar::<_, Uuid>(
            r#"
            SELECT ps.id
            FROM pack_situations ps
            JOIN game_selected_situation_packs gssp ON ps.pack_id = gssp.pack_id
            WHERE gssp.game_id = $1
            "#,
        )
        .bind(game_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(ids)
    }

    async fn get_submission_by_id(
        &self,
        submission_id: Uuid,
    ) -> Result<Option<RoundSubmission>, AppError> {
        let sub = sqlx::query_as::<_, RoundSubmission>(
            r#"
            SELECT id, round_id, user_id, submission_meme_id, submission_situation_id, submitted_at
            FROM round_submissions
            WHERE id = $1
            "#,
        )
        .bind(submission_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(sub)
    }

    async fn get_players_with_submissions(
        &self,
        game_id: Uuid,
        round_id: Option<Uuid>,
    ) -> Result<Vec<PlayerSubmissionState>, AppError> {
        if let Some(r_id) = round_id {
            let players = sqlx::query_as::<_, PlayerSubmissionState>(
                r#"
                SELECT
                    gp.user_id,
                    gp.score,
                    gp.is_ready,
                    gp.handle,
                    EXISTS (
                        SELECT 1
                        FROM round_submissions rs
                        WHERE rs.round_id = $2 AND rs.user_id = gp.user_id
                    ) AS has_submitted
                FROM game_players gp
                WHERE gp.game_id = $1
                ORDER BY gp.joined_at ASC
                "#,
            )
            .bind(game_id)
            .bind(r_id)
            .fetch_all(&self.pool)
            .await?;

            Ok(players)
        } else {
            let players = sqlx::query_as::<_, PlayerSubmissionState>(
                r#"
                SELECT
                    gp.user_id,
                    gp.score,
                    gp.is_ready,
                    gp.handle,
                    false AS has_submitted
                FROM game_players gp
                WHERE gp.game_id = $1
                ORDER BY gp.joined_at ASC
                "#,
            )
            .bind(game_id)
            .fetch_all(&self.pool)
            .await?;

            Ok(players)
        }
    }

    async fn begin(&self) -> Result<Transaction<'static, Postgres>, AppError> {
        let tx = self.pool.begin().await?;
        Ok(tx)
    }

    async fn create_game(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        host_id: Uuid,
        mode: GameMode,
        max_rounds: i32,
        hand_size: i32,
    ) -> Result<Game, AppError> {
        let game = sqlx::query_as::<_, Game>(
            r#"
            INSERT INTO games (host_id, mode, max_rounds, hand_size, status, version)
            VALUES ($1, $2, $3, $4, 'lobby', 1)
            RETURNING id, host_id, mode, status, max_rounds, hand_size, submit_time_limit, vote_time_limit, current_round, version, started_at, finished_at, created_at
            "#,
        )
        .bind(host_id)
        .bind(mode)
        .bind(max_rounds)
        .bind(hand_size)
        .fetch_one(&mut **tx)
        .await?;

        Ok(game)
    }

    async fn add_selected_situation_pack(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        pack_id: Uuid,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            INSERT INTO game_selected_situation_packs (game_id, pack_id)
            VALUES ($1, $2)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(game_id)
        .bind(pack_id)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    async fn add_selected_meme_pack(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        pack_id: Uuid,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            INSERT INTO game_selected_meme_packs (game_id, pack_id)
            VALUES ($1, $2)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(game_id)
        .bind(pack_id)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    async fn find_game_for_update(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
    ) -> Result<Option<Game>, AppError> {
        let game = sqlx::query_as::<_, Game>(
            r#"
            SELECT id, host_id, mode, status, max_rounds, hand_size, submit_time_limit, vote_time_limit, current_round, version, started_at, finished_at, created_at
            FROM games
            WHERE id = $1
            FOR UPDATE
            "#,
        )
        .bind(game_id)
        .fetch_optional(&mut **tx)
        .await?;

        Ok(game)
    }

    async fn increment_game_version(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
    ) -> Result<i64, AppError> {
        let new_version = sqlx::query_scalar::<_, i64>(
            r#"
            UPDATE games
            SET version = version + 1
            WHERE id = $1
            RETURNING version
            "#,
        )
        .bind(game_id)
        .fetch_one(&mut **tx)
        .await?;

        Ok(new_version)
    }

    async fn update_game_status(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        status: GameStatus,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            UPDATE games
            SET status = $2
            WHERE id = $1
            "#,
        )
        .bind(game_id)
        .bind(status)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    async fn get_user_username(&self, user_id: Uuid) -> Result<Option<String>, AppError> {
        // The username may be NULL for guest users. Use a scalar query that returns Option<String>.
        let username = sqlx::query_scalar::<_, Option<String>>(
            "SELECT username FROM users WHERE id = $1",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(username)
    }

    async fn add_player(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        user_id: Uuid,
        is_ready: bool,
        handle: String,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            INSERT INTO game_players (game_id, user_id, score, is_ready, handle)
            VALUES ($1, $2, 0, $3, $4)
            ON CONFLICT (game_id, user_id) DO NOTHING
            "#,
        )
        .bind(game_id)
        .bind(user_id)
        .bind(is_ready)
        .bind(handle)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    async fn update_player_ready(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        user_id: Uuid,
        is_ready: bool,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            UPDATE game_players
            SET is_ready = $3
            WHERE game_id = $1 AND user_id = $2
            "#,
        )
        .bind(game_id)
        .bind(user_id)
        .bind(is_ready)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    async fn get_round_for_update(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        round_id: Uuid,
    ) -> Result<Option<GameRound>, AppError> {
        let round = sqlx::query_as::<_, GameRound>(
            r#"
            SELECT id, game_id, round_number, prompt_situation_id, prompt_meme_id, phase, winner_user_id, phase_expires_at, claimed_at, claimed_by, created_at
            FROM game_rounds
            WHERE id = $1
            FOR UPDATE
            "#,
        )
        .bind(round_id)
        .fetch_optional(&mut **tx)
        .await?;

        Ok(round)
    }

    async fn insert_round(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        round_number: i32,
        prompt_situation_id: Option<Uuid>,
        prompt_meme_id: Option<Uuid>,
        phase: RoundPhase,
        phase_expires_at: Option<DateTime<Utc>>,
    ) -> Result<GameRound, AppError> {
        let round = sqlx::query_as::<_, GameRound>(
            r#"
            INSERT INTO game_rounds (game_id, round_number, prompt_situation_id, prompt_meme_id, phase, phase_expires_at)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id, game_id, round_number, prompt_situation_id, prompt_meme_id, phase, winner_user_id, phase_expires_at, claimed_at, claimed_by, created_at
            "#,
        )
        .bind(game_id)
        .bind(round_number)
        .bind(prompt_situation_id)
        .bind(prompt_meme_id)
        .bind(phase)
        .bind(phase_expires_at)
        .fetch_one(&mut **tx)
        .await?;

        Ok(round)
    }

    async fn check_player_hand_card(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        user_id: Uuid,
        card_id: Uuid,
    ) -> Result<Option<GamePlayerHandCard>, AppError> {
        let hand_card = sqlx::query_as::<_, GamePlayerHandCard>(
            r#"
            SELECT id, game_id, user_id, meme_id, situation_id, is_used
            FROM game_player_hand
            WHERE game_id = $1 AND user_id = $2 AND (meme_id = $3 OR situation_id = $3) AND is_used = false
            "#,
        )
        .bind(game_id)
        .bind(user_id)
        .bind(card_id)
        .fetch_optional(&mut **tx)
        .await?;

        Ok(hand_card)
    }

    async fn mark_card_used(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        hand_card_id: Uuid,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            UPDATE game_player_hand
            SET is_used = true
            WHERE id = $1
            "#,
        )
        .bind(hand_card_id)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    async fn insert_hand_card(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        user_id: Uuid,
        meme_id: Option<Uuid>,
        situation_id: Option<Uuid>,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            INSERT INTO game_player_hand (game_id, user_id, meme_id, situation_id)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(game_id)
        .bind(user_id)
        .bind(meme_id)
        .bind(situation_id)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    async fn insert_submission(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        round_id: Uuid,
        user_id: Uuid,
        meme_id: Option<Uuid>,
        situation_id: Option<Uuid>,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            INSERT INTO round_submissions (round_id, user_id, submission_meme_id, submission_situation_id)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(round_id)
        .bind(user_id)
        .bind(meme_id)
        .bind(situation_id)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    async fn get_round_submissions(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        round_id: Uuid,
    ) -> Result<Vec<RoundSubmission>, AppError> {
        let subs = sqlx::query_as::<_, RoundSubmission>(
            r#"
            SELECT id, round_id, user_id, submission_meme_id, submission_situation_id, submitted_at
            FROM round_submissions
            WHERE round_id = $1
            "#,
        )
        .bind(round_id)
        .fetch_all(&mut **tx)
        .await?;

        Ok(subs)
    }

    async fn get_submissions_count(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        round_id: Uuid,
    ) -> Result<i64, AppError> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM round_submissions
            WHERE round_id = $1
            "#,
        )
        .bind(round_id)
        .fetch_one(&mut **tx)
        .await?;

        Ok(count)
    }

    async fn update_round_phase(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        round_id: Uuid,
        phase: RoundPhase,
        phase_expires_at: Option<DateTime<Utc>>,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            UPDATE game_rounds
            SET phase = $2, phase_expires_at = $3, claimed_at = NULL, claimed_by = NULL
            WHERE id = $1
            "#,
        )
        .bind(round_id)
        .bind(phase)
        .bind(phase_expires_at)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    async fn update_round_winner_and_phase(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        round_id: Uuid,
        winner_user_id: Option<Uuid>,
        phase: RoundPhase,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            UPDATE game_rounds
            SET winner_user_id = $2, phase = $3, phase_expires_at = NULL, claimed_at = NULL, claimed_by = NULL
            WHERE id = $1
            "#,
        )
        .bind(round_id)
        .bind(winner_user_id)
        .bind(phase)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    async fn increment_player_score(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            UPDATE game_players
            SET score = score + 1
            WHERE game_id = $1 AND user_id = $2
            "#,
        )
        .bind(game_id)
        .bind(user_id)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    async fn check_player_voted(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        round_id: Uuid,
        voter_id: Uuid,
    ) -> Result<bool, AppError> {
        let voted = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM round_votes
                WHERE round_id = $1 AND voter_id = $2
            )
            "#,
        )
        .bind(round_id)
        .bind(voter_id)
        .fetch_one(&mut **tx)
        .await?;

        Ok(voted)
    }

    async fn insert_vote(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        round_id: Uuid,
        voter_id: Uuid,
        submission_id: Uuid,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            INSERT INTO round_votes (round_id, voter_id, submission_id)
            VALUES ($1, $2, $3)
            "#,
        )
        .bind(round_id)
        .bind(voter_id)
        .bind(submission_id)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    async fn get_votes_count(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        round_id: Uuid,
    ) -> Result<i64, AppError> {
        let count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*)
            FROM round_votes
            WHERE round_id = $1
            "#,
        )
        .bind(round_id)
        .fetch_one(&mut **tx)
        .await?;

        Ok(count)
    }

    async fn get_votes_by_submission(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        round_id: Uuid,
    ) -> Result<Vec<(Uuid, i64)>, AppError> {
        #[derive(sqlx::FromRow)]
        struct VoteTally {
            submission_id: Uuid,
            vote_count: Option<i64>,
        }

        let rows = sqlx::query_as::<_, VoteTally>(
            r#"
            SELECT submission_id, COUNT(*) AS vote_count
            FROM round_votes
            WHERE round_id = $1
            GROUP BY submission_id
            "#,
        )
        .bind(round_id)
        .fetch_all(&mut **tx)
        .await?;

        let results = rows
            .into_iter()
            .map(|row| (row.submission_id, row.vote_count.unwrap_or(0)))
            .collect();

        Ok(results)
    }

    async fn get_round_scoreboard(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        round_id: Uuid,
    ) -> Result<Vec<(Uuid, i32)>, AppError> {
        #[derive(sqlx::FromRow)]
        struct RoundScoreRow {
            user_id: Uuid,
            vote_count: i64,
        }

        let rows = sqlx::query_as::<_, RoundScoreRow>(
            r#"
            SELECT gp.user_id, COUNT(rv.voter_id) AS vote_count
            FROM game_players gp
            LEFT JOIN round_submissions rs ON rs.round_id = $2 AND rs.user_id = gp.user_id
            LEFT JOIN round_votes rv ON rv.round_id = $2 AND rv.submission_id = rs.id
            WHERE gp.game_id = $1
            GROUP BY gp.user_id, gp.joined_at
            ORDER BY gp.joined_at ASC
            "#,
        )
        .bind(game_id)
        .bind(round_id)
        .fetch_all(&mut **tx)
        .await?;

        let results = rows
            .into_iter()
            .map(|row| (row.user_id, row.vote_count as i32))
            .collect();

        Ok(results)
    }

    async fn get_prompt_details(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        prompt_kind: &str,
        prompt_id: Uuid,
    ) -> Result<(Option<i64>, Option<String>), AppError> {
        if prompt_kind == "situation" {
            #[derive(sqlx::FromRow)]
            struct PromptTextRow {
                prompt_text: String,
            }

            let row = sqlx::query_as::<_, PromptTextRow>(
                r#"
                SELECT prompt_text
                FROM pack_situations
                WHERE id = $1
                "#,
            )
            .bind(prompt_id)
            .fetch_optional(&mut **tx)
            .await?;

            if let Some(r) = row {
                Ok((None, Some(r.prompt_text)))
            } else {
                Err(AppError::NotFound("Situation prompt not found".to_string()))
            }
        } else if prompt_kind == "meme" {
            #[derive(sqlx::FromRow)]
            struct PromptMemeRow {
                media_id: Option<i64>,
            }

            let row = sqlx::query_as::<_, PromptMemeRow>(
                r#"
                SELECT media_id
                FROM pack_memes
                WHERE id = $1
                "#,
            )
            .bind(prompt_id)
            .fetch_optional(&mut **tx)
            .await?;

            if let Some(r) = row {
                Ok((r.media_id, None))
            } else {
                Err(AppError::NotFound("Meme prompt not found".to_string()))
            }
        } else {
            Err(AppError::ValidationError(format!("Invalid prompt kind: {}", prompt_kind)))
        }
    }



    async fn insert_game_event(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        event_id: Uuid,
        game_id: Uuid,
        version: i64,
        event_type: &str,
        payload: serde_json::Value,
    ) -> Result<(), AppError> {
        // UNIQUE (game_id, version) enforces Optimistic Concurrency Control.
        // If a concurrent writer already committed the same version slot we get
        // a 23505 unique violation — map it to Conflict so the caller can
        // surface a meaningful HTTP 409 rather than an opaque 500.
        sqlx::query(
            r#"
            INSERT INTO game_events (id, game_id, version, type, payload)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(event_id)
        .bind(game_id)
        .bind(version)
        .bind(event_type)
        .bind(payload)
        .execute(&mut **tx)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db_err) = e {
                if db_err.code().as_deref() == Some("23505") {
                    return AppError::Conflict(format!(
                        "Concurrent modification detected for game {} at version {}. \
                         Please retry the operation.",
                        game_id, version
                    ));
                }
            }
            AppError::DatabaseError(e)
        })?;

        Ok(())
    }

    async fn update_game_current_round(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        new_round: i32,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            UPDATE games
            SET current_round = $1
            WHERE id = $2
            "#,
        )
        .bind(new_round)
        .bind(game_id)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }



    async fn insert_meme_pack(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        author_id: Uuid,
        name: &str,
        description: Option<&str>,
        language_code: LanguageCode,
        safety_level: ContentSafetyLevel,
        is_public: bool,
    ) -> Result<Uuid, AppError> {
        let pack_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO meme_packs (author_id, name, description, language_code, safety_level, is_public)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            "#,
        )
        .bind(author_id)
        .bind(name)
        .bind(description)
        .bind(language_code)
        .bind(safety_level)
        .bind(is_public)
        .fetch_one(&mut **tx)
        .await?;

        Ok(pack_id)
    }

    async fn insert_pack_meme(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        pack_id: Uuid,
        media_id: i64,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            INSERT INTO pack_memes (pack_id, media_id)
            VALUES ($1, $2)
            "#,
        )
        .bind(pack_id)
        .bind(media_id)
        .execute(&mut **tx)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db_err) = e {
                if db_err.code().as_deref() == Some("23505") {
                    return AppError::Conflict(format!(
                        "Media asset {} is already in this pack",
                        media_id
                    ));
                }
            }
            AppError::DatabaseError(e)
        })?;

        Ok(())
    }

    async fn find_meme_pack(&self, pack_id: Uuid) -> Result<Option<MemePack>, AppError> {
        let pack = sqlx::query_as::<_, MemePack>(
            r#"
            SELECT id, author_id, name, description, language_code, safety_level, is_public, created_at
            FROM meme_packs
            WHERE id = $1
            "#,
        )
        .bind(pack_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(pack)
    }

    async fn list_meme_packs(&self, author_id: Uuid) -> Result<Vec<MemePack>, AppError> {
        let packs = sqlx::query_as::<_, MemePack>(
            r#"
            SELECT id, author_id, name, description, language_code, safety_level, is_public, created_at
            FROM meme_packs
            WHERE is_public = true OR author_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(author_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(packs)
    }

    async fn list_user_meme_packs(&self, author_id: Uuid) -> Result<Vec<MemePack>, AppError> {
        let packs = sqlx::query_as::<_, MemePack>(
            r#"
            SELECT id, author_id, name, description, language_code, safety_level, is_public, created_at
            FROM meme_packs
            WHERE author_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(author_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(packs)
    }

    async fn get_pack_memes_list(&self, pack_id: Uuid) -> Result<Vec<PackMeme>, AppError> {
        let memes = sqlx::query_as::<_, PackMeme>(
            r#"
            SELECT id, pack_id, media_id
            FROM pack_memes
            WHERE pack_id = $1
            ORDER BY id ASC
            "#,
        )
        .bind(pack_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(memes)
    }

    async fn update_meme_pack(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        pack_id: Uuid,
        name: &str,
        description: Option<&str>,
        language_code: LanguageCode,
        safety_level: ContentSafetyLevel,
        is_public: bool,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            UPDATE meme_packs
            SET name = $2, description = $3, language_code = $4, safety_level = $5, is_public = $6
            WHERE id = $1
            "#,
        )
        .bind(pack_id)
        .bind(name)
        .bind(description)
        .bind(language_code)
        .bind(safety_level)
        .bind(is_public)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    async fn delete_meme_pack(&self, tx: &mut Transaction<'_, Postgres>, pack_id: Uuid) -> Result<(), AppError> {
        sqlx::query(
            r#"
            DELETE FROM meme_packs
            WHERE id = $1
            "#,
        )
        .bind(pack_id)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    async fn find_pack_meme_by_id(&self, meme_id: Uuid) -> Result<Option<PackMeme>, AppError> {
        let meme = sqlx::query_as::<_, PackMeme>(
            r#"
            SELECT id, pack_id, media_id
            FROM pack_memes
            WHERE id = $1
            "#,
        )
        .bind(meme_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(meme)
    }

    async fn delete_pack_meme(&self, tx: &mut Transaction<'_, Postgres>, meme_id: Uuid) -> Result<(), AppError> {
        sqlx::query(
            r#"
            DELETE FROM pack_memes
            WHERE id = $1
            "#,
        )
        .bind(meme_id)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    // Situation packs
    async fn insert_situation_pack(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        author_id: Uuid,
        name: &str,
        description: Option<&str>,
        language_code: LanguageCode,
        safety_level: ContentSafetyLevel,
        is_public: bool,
    ) -> Result<Uuid, AppError> {
        let pack_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO situation_packs (author_id, name, description, language_code, safety_level, is_public)
            VALUES ($1, $2, $3, $4, $5, $6)
            RETURNING id
            "#,
        )
        .bind(author_id)
        .bind(name)
        .bind(description)
        .bind(language_code)
        .bind(safety_level)
        .bind(is_public)
        .fetch_one(&mut **tx)
        .await?;

        Ok(pack_id)
    }

    async fn insert_pack_situation(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        pack_id: Uuid,
        prompt_text: &str,
    ) -> Result<Uuid, AppError> {
        let sit_id = sqlx::query_scalar::<_, Uuid>(
            r#"
            INSERT INTO pack_situations (pack_id, prompt_text)
            VALUES ($1, $2)
            RETURNING id
            "#,
        )
        .bind(pack_id)
        .bind(prompt_text)
        .fetch_one(&mut **tx)
        .await
        .map_err(|e| {
            if let sqlx::Error::Database(ref db_err) = e {
                if db_err.code().as_deref() == Some("23505") {
                    return AppError::Conflict(format!(
                        "Situation prompt \"{}\" already exists in this pack",
                        prompt_text
                    ));
                }
            }
            AppError::DatabaseError(e)
        })?;

        Ok(sit_id)
    }

    async fn find_situation_pack(&self, pack_id: Uuid) -> Result<Option<SituationPack>, AppError> {
        let pack = sqlx::query_as::<_, SituationPack>(
            r#"
            SELECT id, author_id, name, description, language_code, safety_level, is_public, created_at
            FROM situation_packs
            WHERE id = $1
            "#,
        )
        .bind(pack_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(pack)
    }

    async fn list_situation_packs(&self, author_id: Uuid) -> Result<Vec<SituationPack>, AppError> {
        let packs = sqlx::query_as::<_, SituationPack>(
            r#"
            SELECT id, author_id, name, description, language_code, safety_level, is_public, created_at
            FROM situation_packs
            WHERE is_public = true OR author_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(author_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(packs)
    }

    async fn list_user_situation_packs(&self, author_id: Uuid) -> Result<Vec<SituationPack>, AppError> {
        let packs = sqlx::query_as::<_, SituationPack>(
            r#"
            SELECT id, author_id, name, description, language_code, safety_level, is_public, created_at
            FROM situation_packs
            WHERE author_id = $1
            ORDER BY created_at DESC
            "#,
        )
        .bind(author_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(packs)
    }

    async fn get_pack_situations_list(&self, pack_id: Uuid) -> Result<Vec<PackSituation>, AppError> {
        let situations = sqlx::query_as::<_, PackSituation>(
            r#"
            SELECT id, pack_id, prompt_text
            FROM pack_situations
            WHERE pack_id = $1
            ORDER BY id ASC
            "#,
        )
        .bind(pack_id)
        .fetch_all(&self.pool)
        .await?;

        Ok(situations)
    }

    async fn update_situation_pack(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        pack_id: Uuid,
        name: &str,
        description: Option<&str>,
        language_code: LanguageCode,
        safety_level: ContentSafetyLevel,
        is_public: bool,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            UPDATE situation_packs
            SET name = $2, description = $3, language_code = $4, safety_level = $5, is_public = $6
            WHERE id = $1
            "#,
        )
        .bind(pack_id)
        .bind(name)
        .bind(description)
        .bind(language_code)
        .bind(safety_level)
        .bind(is_public)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    async fn delete_situation_pack(&self, tx: &mut Transaction<'_, Postgres>, pack_id: Uuid) -> Result<(), AppError> {
        sqlx::query(
            r#"
            DELETE FROM situation_packs
            WHERE id = $1
            "#,
        )
        .bind(pack_id)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    async fn find_pack_situation_by_id(&self, situation_id: Uuid) -> Result<Option<PackSituation>, AppError> {
        let sit = sqlx::query_as::<_, PackSituation>(
            r#"
            SELECT id, pack_id, prompt_text
            FROM pack_situations
            WHERE id = $1
            "#,
        )
        .bind(situation_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(sit)
    }

    async fn delete_pack_situation(&self, tx: &mut Transaction<'_, Postgres>, situation_id: Uuid) -> Result<(), AppError> {
        sqlx::query(
            r#"
            DELETE FROM pack_situations
            WHERE id = $1
            "#,
        )
        .bind(situation_id)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }



    async fn start_game(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        started_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            UPDATE games
            SET status = 'playing', started_at = $2, current_round = 1
            WHERE id = $1
            "#,
        )
        .bind(game_id)
        .bind(started_at)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    async fn insert_player_reserve(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        user_id: Uuid,
        draw_order: i32,
        meme_id: Option<Uuid>,
        situation_id: Option<Uuid>,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            INSERT INTO game_player_reserve (game_id, user_id, draw_order, meme_id, situation_id)
            VALUES ($1, $2, $3, $4, $5)
            "#,
        )
        .bind(game_id)
        .bind(user_id)
        .bind(draw_order)
        .bind(meme_id)
        .bind(situation_id)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    async fn insert_content_lock(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        meme_id: Option<Uuid>,
        situation_id: Option<Uuid>,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            INSERT INTO game_content_locks (game_id, meme_id, situation_id)
            VALUES ($1, $2, $3)
            ON CONFLICT DO NOTHING
            "#,
        )
        .bind(game_id)
        .bind(meme_id)
        .bind(situation_id)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    async fn draw_reserve_card(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        user_id: Uuid,
        draw_order: i32,
    ) -> Result<(), AppError> {
        let row = sqlx::query(
            r#"
            UPDATE game_player_reserve
            SET is_drawn = true
            WHERE game_id = $1 AND user_id = $2 AND draw_order = $3 AND is_drawn = false
            RETURNING meme_id, situation_id
            "#,
        )
        .bind(game_id)
        .bind(user_id)
        .bind(draw_order)
        .fetch_optional(&mut **tx)
        .await?;

        if let Some(r) = row {
            use sqlx::Row;
            let meme_id: Option<Uuid> = r.try_get("meme_id").ok();
            let situation_id: Option<Uuid> = r.try_get("situation_id").ok();

            sqlx::query(
                r#"
                INSERT INTO game_player_hand (game_id, user_id, meme_id, situation_id)
                VALUES ($1, $2, $3, $4)
                "#,
            )
            .bind(game_id)
            .bind(user_id)
            .bind(meme_id)
            .bind(situation_id)
            .execute(&mut **tx)
            .await?;
        }

        Ok(())
    }

    async fn activate_next_round(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        round_number: i32,
        phase_expires_at: Option<DateTime<Utc>>,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            UPDATE game_rounds
            SET phase = 'submitting', phase_expires_at = $3, claimed_at = NULL, claimed_by = NULL
            WHERE game_id = $1 AND round_number = $2
            "#,
        )
        .bind(game_id)
        .bind(round_number)
        .bind(phase_expires_at)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    async fn update_game_settings(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        mode: GameMode,
        max_rounds: i32,
        hand_size: i32,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            UPDATE games
            SET mode = $2, max_rounds = $3, hand_size = $4
            WHERE id = $1
            "#,
        )
        .bind(game_id)
        .bind(mode)
        .bind(max_rounds)
        .bind(hand_size)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    async fn clear_selected_situation_packs(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            DELETE FROM game_selected_situation_packs
            WHERE game_id = $1
            "#,
        )
        .bind(game_id)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    async fn clear_selected_meme_packs(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            DELETE FROM game_selected_meme_packs
            WHERE game_id = $1
            "#,
        )
        .bind(game_id)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    async fn delete_game_content_locks(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            DELETE FROM game_content_locks
            WHERE game_id = $1
            "#,
        )
        .bind(game_id)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    async fn is_meme_pack_locked(&self, pack_id: Uuid) -> Result<bool, AppError> {
        let locked = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM game_content_locks gcl
                JOIN pack_memes pm ON pm.id = gcl.meme_id
                WHERE pm.pack_id = $1
            )
            "#
        )
        .bind(pack_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(locked)
    }

    async fn is_situation_pack_locked(&self, pack_id: Uuid) -> Result<bool, AppError> {
        let locked = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM game_content_locks gcl
                JOIN pack_situations ps ON ps.id = gcl.situation_id
                WHERE ps.pack_id = $1
            )
            "#
        )
        .bind(pack_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(locked)
    }

    async fn is_meme_locked(&self, meme_id: Uuid) -> Result<bool, AppError> {
        let locked = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM game_content_locks WHERE meme_id = $1
            )
            "#
        )
        .bind(meme_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(locked)
    }

    async fn is_situation_locked(&self, situation_id: Uuid) -> Result<bool, AppError> {
        let locked = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1 FROM game_content_locks WHERE situation_id = $1
            )
            "#
        )
        .bind(situation_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(locked)
    }

    async fn get_unused_hand_cards(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<GamePlayerHandCard>, AppError> {
        let cards = sqlx::query_as::<_, GamePlayerHandCard>(
            r#"
            SELECT id, game_id, user_id, meme_id, situation_id, is_used
            FROM game_player_hand
            WHERE game_id = $1 AND user_id = $2 AND is_used = false
            "#,
        )
        .bind(game_id)
        .bind(user_id)
        .fetch_all(&mut **tx)
        .await?;

        Ok(cards)
    }

    async fn claim_next_expired_round(
        &self,
        worker_id: Uuid,
        now: DateTime<Utc>,
        stale_timeout: DateTime<Utc>,
    ) -> Result<Option<GameRound>, AppError> {
        let round = sqlx::query_as::<_, GameRound>(
            r#"
            UPDATE game_rounds
            SET claimed_at = $1, claimed_by = $2
            WHERE id IN (
                SELECT id
                FROM game_rounds
                WHERE phase IN ('submitting', 'voting')
                  AND phase_expires_at <= $3
                  AND (claimed_at IS NULL OR claimed_at < $4)
                ORDER BY phase_expires_at ASC
                FOR UPDATE SKIP LOCKED
                LIMIT 1
            )
            RETURNING id, game_id, round_number, prompt_situation_id, prompt_meme_id, phase, winner_user_id, phase_expires_at, claimed_at, claimed_by, created_at
            "#,
        )
        .bind(now)
        .bind(worker_id)
        .bind(now)
        .bind(stale_timeout)
        .fetch_optional(&self.pool)
        .await?;

        Ok(round)
    }
}

