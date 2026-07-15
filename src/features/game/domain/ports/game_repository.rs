use async_trait::async_trait;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;
use chrono::{DateTime, Utc};


use crate::{
    common::http::error::AppError,
    features::game::domain::model::{
        Game, ActiveGame, GameMode, GamePlayer, GamePlayerHandCard, GameRound, GameStatus,
        PlayerSubmissionState, RoundPhase, RoundSubmission, ContentSafetyLevel, LanguageCode,
        MemePack, PackMeme, SituationPack, PackSituation, GamePlayerHandCardWithMedia, RawGameCard,
    },
};

#[async_trait]
pub trait GameRepository: Send + Sync {
    async fn find_game(&self, game_id: Uuid) -> Result<Option<Game>, AppError>;
    async fn find_active_lobby_games(&self) -> Result<Vec<ActiveGame>, AppError>;
    async fn get_players(&self, game_id: Uuid) -> Result<Vec<GamePlayer>, AppError>;
    async fn get_player_hand(&self, game_id: Uuid, user_id: Uuid) -> Result<Vec<RawGameCard>, AppError>;
    async fn get_player_hand_with_media(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<GamePlayerHandCardWithMedia>, AppError>;

    async fn get_current_round(&self, game_id: Uuid) -> Result<Option<GameRound>, AppError>;
    async fn get_round(&self, round_id: Uuid) -> Result<Option<GameRound>, AppError>;
    async fn get_prompt_card(
        &self,
        situation_id: Option<Uuid>,
        meme_id: Option<Uuid>,
    ) -> Result<Option<RawGameCard>, AppError>;
    async fn get_available_memes(&self, game_id: Uuid) -> Result<Vec<Uuid>, AppError>;
    async fn get_available_situations(&self, game_id: Uuid) -> Result<Vec<Uuid>, AppError>;
    async fn get_submission_by_id(&self, submission_id: Uuid)
        -> Result<Option<RoundSubmission>, AppError>;
    async fn get_players_with_submissions(
        &self,
        game_id: Uuid,
        round_id: Option<Uuid>,
    ) -> Result<Vec<PlayerSubmissionState>, AppError>;

    async fn begin(&self) -> Result<Transaction<'static, Postgres>, AppError>;

    async fn create_game(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        host_id: Uuid,
        mode: GameMode,
        max_rounds: i32,
        hand_size: i32,
    ) -> Result<Game, AppError>;

    async fn add_selected_situation_pack(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        pack_id: Uuid,
    ) -> Result<(), AppError>;

    async fn add_selected_meme_pack(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        pack_id: Uuid,
    ) -> Result<(), AppError>;

    async fn find_game_for_update(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
    ) -> Result<Option<Game>, AppError>;

    async fn increment_game_version(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
    ) -> Result<i64, AppError>;

    async fn update_game_status(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        status: GameStatus,
    ) -> Result<(), AppError>;

    async fn get_user_username(&self, user_id: Uuid) -> Result<Option<String>, AppError>;

    async fn add_player(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        user_id: Uuid,
        is_ready: bool,
        nickname: String,
    ) -> Result<(), AppError>;

    async fn update_player_ready(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        user_id: Uuid,
        is_ready: bool,
    ) -> Result<(), AppError>;

    async fn get_round_for_update(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        round_id: Uuid,
    ) -> Result<Option<GameRound>, AppError>;

    async fn insert_round(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        round_number: i32,
        prompt_situation_id: Option<Uuid>,
        prompt_meme_id: Option<Uuid>,
        phase: RoundPhase,
        phase_expires_at: Option<DateTime<Utc>>,
    ) -> Result<GameRound, AppError>;

    async fn activate_next_round(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        round_number: i32,
        phase_expires_at: Option<DateTime<Utc>>,
    ) -> Result<(), AppError>;


    async fn check_player_hand_card(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        user_id: Uuid,
        card_id: Uuid,
    ) -> Result<Option<GamePlayerHandCard>, AppError>;

    async fn mark_card_used(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        hand_card_id: Uuid,
    ) -> Result<(), AppError>;

    async fn insert_hand_card(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        user_id: Uuid,
        meme_id: Option<Uuid>,
        situation_id: Option<Uuid>,
    ) -> Result<(), AppError>;

    async fn insert_submission(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        round_id: Uuid,
        user_id: Uuid,
        meme_id: Option<Uuid>,
        situation_id: Option<Uuid>,
    ) -> Result<(), AppError>;

    async fn get_submissions_count(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        round_id: Uuid,
    ) -> Result<i64, AppError>;

    async fn update_round_phase(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        round_id: Uuid,
        phase: RoundPhase,
        phase_expires_at: Option<DateTime<Utc>>,
    ) -> Result<(), AppError>;
    async fn get_round_submissions(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        round_id: Uuid,
    ) -> Result<Vec<RoundSubmission>, AppError>;

    async fn update_round_winner_and_phase(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        round_id: Uuid,
        winner_user_id: Option<Uuid>,
        phase: RoundPhase,
    ) -> Result<(), AppError>;

    async fn increment_player_score(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        user_id: Uuid,
    ) -> Result<(), AppError>;

    async fn check_player_voted(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        round_id: Uuid,
        voter_id: Uuid,
    ) -> Result<bool, AppError>;

    async fn insert_vote(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        round_id: Uuid,
        voter_id: Uuid,
        submission_id: Uuid,
    ) -> Result<(), AppError>;

    async fn get_votes_count(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        round_id: Uuid,
    ) -> Result<i64, AppError>;

    async fn get_votes_by_submission(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        round_id: Uuid,
    ) -> Result<Vec<(Uuid, i64)>, AppError>;

    async fn get_round_scoreboard(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        round_id: Uuid,
    ) -> Result<Vec<(Uuid, i32)>, AppError>;

    async fn get_prompt_details(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        prompt_kind: &str,
        prompt_id: Uuid,
    ) -> Result<(Option<i64>, Option<String>), AppError>;

    /// Insert an event into `game_events`.
    ///
    /// The table has `UNIQUE (game_id, version)` so two concurrent writers
    /// racing for the same version slot will get a Postgres `23505` unique
    /// violation.  The implementation must map that to `AppError::Conflict`
    /// so the caller can retry or surface a meaningful error to the client.
    async fn insert_game_event(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        event_id: Uuid,
        game_id: Uuid,
        version: i64,
        event_type: &str,
        payload: serde_json::Value,
    ) -> Result<(), AppError>;

    /// Advance `current_round` on the games read-model.
    async fn update_game_current_round(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        new_round: i32,
    ) -> Result<(), AppError>;



    async fn insert_meme_pack(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        author_id: Uuid,
        name: &str,
        description: Option<&str>,
        language_code: LanguageCode,
        safety_level: ContentSafetyLevel,
        is_public: bool,
    ) -> Result<Uuid, AppError>;

    async fn insert_pack_meme(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        pack_id: Uuid,
        media_id: i64,
    ) -> Result<(), AppError>;

    async fn find_meme_pack(&self, pack_id: Uuid) -> Result<Option<MemePack>, AppError>;
    async fn list_meme_packs(&self, author_id: Uuid) -> Result<Vec<MemePack>, AppError>;
    async fn list_user_meme_packs(&self, author_id: Uuid) -> Result<Vec<MemePack>, AppError>;
    async fn get_pack_memes_list(&self, pack_id: Uuid) -> Result<Vec<PackMeme>, AppError>;
    async fn update_meme_pack(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        pack_id: Uuid,
        name: &str,
        description: Option<&str>,
        language_code: LanguageCode,
        safety_level: ContentSafetyLevel,
        is_public: bool,
    ) -> Result<(), AppError>;
    async fn delete_meme_pack(&self, tx: &mut Transaction<'_, Postgres>, pack_id: Uuid) -> Result<(), AppError>;
    async fn find_pack_meme_by_id(&self, meme_id: Uuid) -> Result<Option<PackMeme>, AppError>;
    async fn delete_pack_meme(&self, tx: &mut Transaction<'_, Postgres>, meme_id: Uuid) -> Result<(), AppError>;

    async fn insert_situation_pack(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        author_id: Uuid,
        name: &str,
        description: Option<&str>,
        language_code: LanguageCode,
        safety_level: ContentSafetyLevel,
        is_public: bool,
    ) -> Result<Uuid, AppError>;
    async fn insert_pack_situation(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        pack_id: Uuid,
        prompt_text: &str,
    ) -> Result<Uuid, AppError>;
    async fn find_situation_pack(&self, pack_id: Uuid) -> Result<Option<SituationPack>, AppError>;
    async fn list_situation_packs(&self, author_id: Uuid) -> Result<Vec<SituationPack>, AppError>;
    async fn list_user_situation_packs(&self, author_id: Uuid) -> Result<Vec<SituationPack>, AppError>;
    async fn get_pack_situations_list(&self, pack_id: Uuid) -> Result<Vec<PackSituation>, AppError>;
    async fn update_situation_pack(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        pack_id: Uuid,
        name: &str,
        description: Option<&str>,
        language_code: LanguageCode,
        safety_level: ContentSafetyLevel,
        is_public: bool,
    ) -> Result<(), AppError>;
    async fn delete_situation_pack(&self, tx: &mut Transaction<'_, Postgres>, pack_id: Uuid) -> Result<(), AppError>;
    async fn find_pack_situation_by_id(&self, situation_id: Uuid) -> Result<Option<PackSituation>, AppError>;
    async fn delete_pack_situation(&self, tx: &mut Transaction<'_, Postgres>, situation_id: Uuid) -> Result<(), AppError>;



    async fn start_game(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        started_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), AppError>;

    async fn insert_player_reserve(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        user_id: Uuid,
        draw_order: i32,
        meme_id: Option<Uuid>,
        situation_id: Option<Uuid>,
    ) -> Result<(), AppError>;

    async fn insert_content_lock(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        meme_id: Option<Uuid>,
        situation_id: Option<Uuid>,
    ) -> Result<(), AppError>;

    async fn draw_reserve_card(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        user_id: Uuid,
        draw_order: i32,
    ) -> Result<(), AppError>;

    async fn update_game_settings(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        mode: GameMode,
        max_rounds: i32,
        hand_size: i32,
    ) -> Result<(), AppError>;

    async fn clear_selected_situation_packs(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
    ) -> Result<(), AppError>;

    async fn clear_selected_meme_packs(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
    ) -> Result<(), AppError>;

    async fn delete_game_content_locks(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
    ) -> Result<(), AppError>;

    async fn is_meme_pack_locked(&self, pack_id: Uuid) -> Result<bool, AppError>;
    async fn is_situation_pack_locked(&self, pack_id: Uuid) -> Result<bool, AppError>;
    async fn is_meme_locked(&self, meme_id: Uuid) -> Result<bool, AppError>;
    async fn is_situation_locked(&self, situation_id: Uuid) -> Result<bool, AppError>;

    async fn get_unused_hand_cards(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        user_id: Uuid,
    ) -> Result<Vec<GamePlayerHandCard>, AppError>;

    async fn claim_next_expired_round(
        &self,
        worker_id: Uuid,
        now: DateTime<Utc>,
        stale_timeout: DateTime<Utc>,
    ) -> Result<Option<GameRound>, AppError>;
}

