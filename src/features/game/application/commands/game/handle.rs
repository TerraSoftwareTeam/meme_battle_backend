use uuid::Uuid;
use crate::{
    common::http::error::AppError,
    features::game::domain::model::game::GamePlayer,
};

pub fn resolve_handle(
    user_id: Uuid,
    requested_handle: Option<String>,
    user_nickname: String,
    existing_players: &[GamePlayer],
) -> Result<String, AppError> {
    let (proposed_handle, is_explicit) = match requested_handle {
        Some(h) if !h.trim().is_empty() => (h, true),
        _ => (user_nickname, false),
    };

    let conflict = existing_players.iter().any(|p| p.handle == proposed_handle);
    if conflict {
        if is_explicit {
            return Err(AppError::Conflict("handle already taken in this lobby".to_string()));
        } else {
            return Ok(user_id.to_string());
        }
    }

    Ok(proposed_handle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_mock_player(user_id: Uuid, handle: &str) -> GamePlayer {
        GamePlayer {
            game_id: Uuid::new_v4(),
            user_id,
            score: 0,
            is_ready: false,
            handle: handle.to_string(),
            joined_at: Utc::now(),
        }
    }

    #[test]
    fn test_resolve_handle_explicit_success() {
        let user_id = Uuid::new_v4();
        let existing = vec![
            make_mock_player(Uuid::new_v4(), "Alice"),
        ];

        let result = resolve_handle(
            user_id,
            Some("Bob".to_string()),
            "Bobby".to_string(),
            &existing,
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Bob");
    }

    #[test]
    fn test_resolve_handle_explicit_conflict() {
        let user_id = Uuid::new_v4();
        let existing = vec![
            make_mock_player(Uuid::new_v4(), "Alice"),
        ];

        let result = resolve_handle(
            user_id,
            Some("Alice".to_string()),
            "Bobby".to_string(),
            &existing,
        );

        assert!(result.is_err());
        match result.unwrap_err() {
            AppError::Conflict(msg) => assert_eq!(msg, "handle already taken in this lobby"),
            _ => panic!("Expected conflict error"),
        }
    }

    #[test]
    fn test_resolve_handle_default_no_conflict() {
        let user_id = Uuid::new_v4();
        let existing = vec![
            make_mock_player(Uuid::new_v4(), "Alice"),
        ];

        let result = resolve_handle(
            user_id,
            None,
            "Bob".to_string(),
            &existing,
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Bob");
    }

    #[test]
    fn test_resolve_handle_default_conflict_fallback_to_uuid() {
        let user_id = Uuid::new_v4();
        let existing = vec![
            make_mock_player(Uuid::new_v4(), "Alice"),
        ];

        let result = resolve_handle(
            user_id,
            None,
            "Alice".to_string(),
            &existing,
        );

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), user_id.to_string());
    }

    #[test]
    fn test_resolve_handle_conflict_chain() {
        let user_a_id = Uuid::new_v4();
        let user_b_id = Uuid::new_v4();

        let existing_empty = vec![];
        let handle_a = resolve_handle(
            user_a_id,
            Some("Bob".to_string()),
            "Alice".to_string(),
            &existing_empty,
        ).unwrap();
        assert_eq!(handle_a, "Bob");

        let player_a = make_mock_player(user_a_id, &handle_a);
        let existing_with_a = vec![player_a];

        let handle_b = resolve_handle(
            user_b_id,
            None,
            "Bob".to_string(),
            &existing_with_a,
        ).unwrap();

        assert_eq!(handle_b, user_b_id.to_string());
    }
}
