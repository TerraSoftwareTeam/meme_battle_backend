use serde_json::json;
use uuid::Uuid;

use crate::{
    common::http::error::AppError,
    features::game::domain::ports::GameRepository,
};

pub fn calculate_partition(game_id: Uuid) -> i32 {
    let bytes = game_id.as_bytes();
    (u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) % 1000) as i32
}

pub async fn publish_event(
    repo: &dyn GameRepository,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    game_id: Uuid,
    new_version: i64,
    event_type: &str,
    event_payload: serde_json::Value,
) -> Result<(), AppError> {
    let event_id = Uuid::new_v4();

    // Write to game_events
    repo.insert_game_event(
        tx,
        event_id,
        game_id,
        new_version,
        event_type,
        event_payload.clone(),
    )
    .await?;

    // Centrifugo outbox payload
    let centrifugo_payload = json!({
        "channel": format!("game:{}", game_id),
        "data": {
            "eventId": event_id.to_string(),
            "version": new_version,
            "type": event_type,
            "payload": event_payload
        },
        "idempotency_key": event_id.to_string()
    });

    let partition = calculate_partition(game_id);

    // Write to centrifugo_outbox
    repo.insert_centrifugo_outbox(tx, "publish", centrifugo_payload, partition)
        .await?;

    Ok(())
}
