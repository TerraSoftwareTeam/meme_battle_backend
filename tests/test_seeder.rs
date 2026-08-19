use std::fs;
use sqlx::postgres::PgPoolOptions;
use uuid::Uuid;
use meme_battle_backend::{
    common::{
        app::{bootstrap::run_database_migrations, config::Config},
        http::error::AppError,
        seeder::Seeder,
    },
    features::game::{
        ContentSafetyLevel, GameMode, GameRepository, GameRepositoryImpl, LanguageCode,
    },
};


#[tokio::test]
async fn test_seeder_situations_and_memes_lifecycle() {
    dotenvy::dotenv().ok();
    let _ = tracing_subscriber::fmt::try_init();

    // 1. Setup DB pool
    let config = Config::from_env().unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .min_connections(1)
        .connect(&config.database_url)
        .await
        .unwrap();
    run_database_migrations(&pool).await.unwrap();

    let repo = GameRepositoryImpl::new(pool.clone());
    let seeder = Seeder::new(pool.clone(), config.clone());

    // 2. Prepare temporary test seed directory
    let temp_root = std::env::temp_dir().join(format!("meme_test_seeds_{}", Uuid::new_v4()));
    let situations_dir = temp_root.join("official").join("situations");
    let memes_dir = temp_root.join("official").join("memes");
    let assets_dir = memes_dir.join("assets");

    fs::create_dir_all(&situations_dir).unwrap();
    fs::create_dir_all(&assets_dir).unwrap();

    // Copy a sample asset for memes
    let sample_image_bytes = b"fake_png_image_binary_data_for_test";
    let test_meme_path = assets_dir.join("test_meme.png");
    fs::write(&test_meme_path, sample_image_bytes).unwrap();

    let ru_pack_id = Uuid::new_v4();
    let en_pack_id = Uuid::new_v4();
    let meme_ru_pack_id = Uuid::new_v4();
    let meme_en_pack_id = Uuid::new_v4();

    // Write initial situation configs
    let ru_situations_json = format!(r#"{{
        "pack_id": "{}",
        "name": "Test Situations RU",
        "description": "RU Situations",
        "language_code": "ru",
        "safety_level": "family_friendly",
        "is_public": true,
        "items": [
            "Ситуация 1",
            "Ситуация 2",
            "Ситуация 3"
        ]
    }}"#, ru_pack_id);

    let en_situations_json = format!(r#"{{
        "pack_id": "{}",
        "name": "Test Situations EN",
        "description": "EN Situations",
        "language_code": "en",
        "safety_level": "family_friendly",
        "is_public": true,
        "items": [
            {{ "prompt_text": "Situation 1" }},
            {{ "prompt_text": "Situation 2" }}
        ]
    }}"#, en_pack_id);

    // Write initial meme configs
    let ru_memes_json = format!(r#"{{
        "pack_id": "{}",
        "name": "Test Memes RU",
        "description": "RU Memes",
        "language_code": "ru",
        "safety_level": "family_friendly",
        "is_public": true,
        "items": [
            "assets/test_meme.png"
        ]
    }}"#, meme_ru_pack_id);

    let en_memes_json = format!(r#"{{
        "pack_id": "{}",
        "name": "Test Memes EN",
        "description": "EN Memes",
        "language_code": "en",
        "safety_level": "family_friendly",
        "is_public": true,
        "items": [
            {{ "file": "assets/test_meme.png" }}
        ]
    }}"#, meme_en_pack_id);

    fs::write(situations_dir.join("ru.json"), ru_situations_json).unwrap();
    fs::write(situations_dir.join("en.json"), en_situations_json).unwrap();
    fs::write(memes_dir.join("ru.json"), ru_memes_json).unwrap();
    fs::write(memes_dir.join("en.json"), en_memes_json).unwrap();

    // 3. Initial sync
    seeder.sync_all(&temp_root).await.unwrap();

    // Verify situation packs in DB
    let ru_pack = repo.find_situation_pack(ru_pack_id).await.unwrap().expect("RU pack should exist");
    assert_eq!(ru_pack.name, "Test Situations RU");

    let en_pack = repo.find_situation_pack(en_pack_id).await.unwrap().expect("EN pack should exist");
    assert_eq!(en_pack.name, "Test Situations EN");

    let ru_situations = repo.get_pack_situations_list(ru_pack_id).await.unwrap();
    assert_eq!(ru_situations.len(), 3);
    assert!(ru_situations.iter().all(|s| s.is_active));

    let en_situations = repo.get_pack_situations_list(en_pack_id).await.unwrap();
    assert_eq!(en_situations.len(), 2);
    assert!(en_situations.iter().all(|s| s.is_active));

    // Verify meme packs in DB
    let ru_meme_pack = repo.find_meme_pack(meme_ru_pack_id).await.unwrap().expect("RU meme pack should exist");
    assert_eq!(ru_meme_pack.name, "Test Memes RU");

    let ru_memes = repo.get_pack_memes_list(meme_ru_pack_id).await.unwrap();
    assert_eq!(ru_memes.len(), 1);
    assert!(ru_memes[0].is_active);

    // 4. Idempotence test (re-running immediately)
    seeder.sync_all(&temp_root).await.unwrap();

    let ru_situations_second = repo.get_pack_situations_list(ru_pack_id).await.unwrap();
    assert_eq!(ru_situations_second.len(), 3);

    // 5. Modification and deactivation test
    // Remove "Ситуация 3" and add "Ситуация 4" in RU pack
    let updated_ru_json = format!(r#"{{
        "pack_id": "{}",
        "name": "Test Situations RU Updated",
        "description": "RU Situations Updated",
        "language_code": "ru",
        "safety_level": "family_friendly",
        "is_public": true,
        "items": [
            "Ситуация 1",
            "Ситуация 2",
            "Ситуация 4"
        ]
    }}"#, ru_pack_id);
    fs::write(situations_dir.join("ru.json"), updated_ru_json).unwrap();

    seeder.sync_all(&temp_root).await.unwrap();

    // get_pack_situations_list filters by is_active = true
    let active_ru_situations = repo.get_pack_situations_list(ru_pack_id).await.unwrap();
    assert_eq!(active_ru_situations.len(), 3);
    let prompts: Vec<String> = active_ru_situations.into_iter().map(|s| s.prompt_text).collect();
    assert!(prompts.contains(&"Ситуация 1".to_string()));
    assert!(prompts.contains(&"Ситуация 2".to_string()));
    assert!(prompts.contains(&"Ситуация 4".to_string()));
    assert!(!prompts.contains(&"Ситуация 3".to_string()));

    // Verify in raw SQL that "Ситуация 3" is still in DB with is_active = false
    #[derive(sqlx::FromRow)]
    struct RawRow {
        prompt_text: String,
        is_active: bool,
    }

    let all_db_rows = sqlx::query_as::<_, RawRow>(
        "SELECT prompt_text, is_active FROM pack_situations WHERE pack_id = $1 ORDER BY prompt_text"
    )
    .bind(ru_pack_id)
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(all_db_rows.len(), 4);
    let sit3 = all_db_rows.iter().find(|r| r.prompt_text == "Ситуация 3").unwrap();
    assert!(!sit3.is_active, "Deactivated item should have is_active = false");

    // Clean up temp test directory
    let _ = fs::remove_dir_all(temp_root);
}

#[tokio::test]
async fn test_seeder_reactivation_and_in_config_duplicates() {
    dotenvy::dotenv().ok();
    let config = Config::from_env().unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .min_connections(1)
        .connect(&config.database_url)
        .await
        .unwrap();
    run_database_migrations(&pool).await.unwrap();

    let repo = GameRepositoryImpl::new(pool.clone());
    let seeder = Seeder::new(pool.clone(), config.clone());

    let temp_root = std::env::temp_dir().join(format!("meme_test_react_{}", Uuid::new_v4()));
    let situations_dir = temp_root.join("official").join("situations");
    fs::create_dir_all(&situations_dir).unwrap();

    let pack_id = Uuid::new_v4();

    // Step 1: Initial with Item A, Item B, and duplicate "Item A" within same file
    let config_v1 = format!(r#"{{
        "pack_id": "{}",
        "name": "Reactivation Test",
        "language_code": "en",
        "items": [
            "Item A",
            "Item B",
            "Item A",
            "   Item A   "
        ]
    }}"#, pack_id);
    fs::write(situations_dir.join("en.json"), config_v1).unwrap();

    seeder.sync_all(&temp_root).await.unwrap();

    let items_v1 = repo.get_pack_situations_list(pack_id).await.unwrap();
    assert_eq!(items_v1.len(), 2, "Duplicates in config must be deduplicated to 2 items");

    let item_a_id = items_v1.iter().find(|i| i.prompt_text == "Item A").unwrap().id;

    // Step 2: Deactivate Item A (remove from config)
    let config_v2 = format!(r#"{{
        "pack_id": "{}",
        "name": "Reactivation Test",
        "language_code": "en",
        "items": [
            "Item B"
        ]
    }}"#, pack_id);
    fs::write(situations_dir.join("en.json"), config_v2).unwrap();

    seeder.sync_all(&temp_root).await.unwrap();
    let items_v2 = repo.get_pack_situations_list(pack_id).await.unwrap();
    assert_eq!(items_v2.len(), 1);
    assert_eq!(items_v2[0].prompt_text, "Item B");

    // Step 3: Re-add Item A back (must reactivate existing row without changing its ID)
    let config_v3 = format!(r#"{{
        "pack_id": "{}",
        "name": "Reactivation Test",
        "language_code": "en",
        "items": [
            "Item A",
            "Item B"
        ]
    }}"#, pack_id);
    fs::write(situations_dir.join("en.json"), config_v3).unwrap();

    seeder.sync_all(&temp_root).await.unwrap();
    let items_v3 = repo.get_pack_situations_list(pack_id).await.unwrap();
    assert_eq!(items_v3.len(), 2);
    let item_a_reloaded = items_v3.iter().find(|i| i.prompt_text == "Item A").unwrap();
    assert_eq!(item_a_reloaded.id, item_a_id, "Reactivated item must preserve original UUID");
    assert!(item_a_reloaded.is_active);

    let _ = fs::remove_dir_all(temp_root);
}

#[tokio::test]
async fn test_seeder_missing_asset_fails_gracefully() {
    dotenvy::dotenv().ok();
    let config = Config::from_env().unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .min_connections(1)
        .connect(&config.database_url)
        .await
        .unwrap();
    run_database_migrations(&pool).await.unwrap();

    let seeder = Seeder::new(pool.clone(), config.clone());

    let temp_root = std::env::temp_dir().join(format!("meme_test_missing_{}", Uuid::new_v4()));
    let memes_dir = temp_root.join("official").join("memes");
    fs::create_dir_all(&memes_dir).unwrap();

    // Reference a non-existent asset file
    let invalid_config = r#"{
        "name": "Broken Meme Pack",
        "language_code": "en",
        "items": [
            "assets/definitely_missing_file_12345.png"
        ]
    }"#;
    fs::write(memes_dir.join("broken.json"), invalid_config).unwrap();

    let result = seeder.sync_all(&temp_root).await;
    assert!(result.is_err(), "Seeder must return Err when an asset file is missing");

    match result.unwrap_err() {
        AppError::NotFound(msg) => {
            assert!(msg.contains("definitely_missing_file_12345.png"));
        }
        other => panic!("Expected AppError::NotFound, got: {:?}", other),
    }

    let _ = fs::remove_dir_all(temp_root);
}

#[tokio::test]
async fn test_seeder_pack_metadata_updates() {
    dotenvy::dotenv().ok();
    let config = Config::from_env().unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .min_connections(1)
        .connect(&config.database_url)
        .await
        .unwrap();
    run_database_migrations(&pool).await.unwrap();

    let repo = GameRepositoryImpl::new(pool.clone());
    let seeder = Seeder::new(pool.clone(), config.clone());

    let temp_root = std::env::temp_dir().join(format!("meme_test_meta_{}", Uuid::new_v4()));
    let situations_dir = temp_root.join("official").join("situations");
    fs::create_dir_all(&situations_dir).unwrap();

    let pack_id = Uuid::new_v4();

    let config_initial = format!(r#"{{
        "pack_id": "{}",
        "name": "Initial Name",
        "description": "Initial Description",
        "language_code": "en",
        "safety_level": "family_friendly",
        "is_public": true,
        "items": ["Sample 1"]
    }}"#, pack_id);
    fs::write(situations_dir.join("en.json"), config_initial).unwrap();

    seeder.sync_all(&temp_root).await.unwrap();

    let pack1 = repo.find_situation_pack(pack_id).await.unwrap().unwrap();
    assert_eq!(pack1.name, "Initial Name");
    assert_eq!(pack1.safety_level, ContentSafetyLevel::FamilyFriendly);
    assert!(pack1.is_public);

    // Update metadata in config
    let config_updated = format!(r#"{{
        "pack_id": "{}",
        "name": "Updated Name",
        "description": "Updated Description",
        "language_code": "ru",
        "safety_level": "spicy",
        "is_public": false,
        "items": ["Sample 1"]
    }}"#, pack_id);
    fs::write(situations_dir.join("en.json"), config_updated).unwrap();

    seeder.sync_all(&temp_root).await.unwrap();

    let pack2 = repo.find_situation_pack(pack_id).await.unwrap().unwrap();
    assert_eq!(pack2.name, "Updated Name");
    assert_eq!(pack2.description.as_deref(), Some("Updated Description"));
    assert_eq!(pack2.language_code, LanguageCode::Ru);
    assert_eq!(pack2.safety_level, ContentSafetyLevel::Spicy);
    assert!(!pack2.is_public);

    let _ = fs::remove_dir_all(temp_root);
}

#[tokio::test]
async fn test_seeder_game_queries_filter_out_deactivated_cards() {
    dotenvy::dotenv().ok();
    let config = Config::from_env().unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .min_connections(1)
        .connect(&config.database_url)
        .await
        .unwrap();
    run_database_migrations(&pool).await.unwrap();

    let repo = GameRepositoryImpl::new(pool.clone());

    // 1. Create a dedicated situation pack and meme pack
    let mut tx = pool.begin().await.unwrap();
    let author_id = uuid::Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    
    let sit_pack_id = repo.insert_situation_pack(
        &mut tx,
        author_id,
        "Game Filter Test Sit Pack",
        None,
        LanguageCode::En,
        ContentSafetyLevel::FamilyFriendly,
        true,
    ).await.unwrap();

    let active_sit_id = repo.insert_pack_situation(&mut tx, sit_pack_id, "Active Situation").await.unwrap();
    let deact_sit_id = repo.insert_pack_situation(&mut tx, sit_pack_id, "Deactivated Situation").await.unwrap();

    tx.commit().await.unwrap();

    // Deactivate the second situation directly in DB
    sqlx::query("UPDATE pack_situations SET is_active = false WHERE id = $1")
        .bind(deact_sit_id)
        .execute(&pool)
        .await
        .unwrap();

    // Create a game selecting this situation pack
    let mut tx = pool.begin().await.unwrap();
    let host_id = author_id;
    let game = repo.create_game(
        &mut tx,
        host_id,
        "Test Game".to_string(),
        GameMode::SituationToMeme,
        3,
        5,
    ).await.unwrap();

    repo.add_selected_situation_pack(&mut tx, game.id, sit_pack_id).await.unwrap();
    tx.commit().await.unwrap();

    // Query available situations for game card draws
    let available_situations = repo.get_available_situations(game.id).await.unwrap();

    assert!(available_situations.contains(&active_sit_id), "Active situation must be available in game");
    assert!(!available_situations.contains(&deact_sit_id), "Deactivated situation must NEVER be drawn in game");
}

#[tokio::test]
async fn test_seeder_custom_admin_user_id() {
    dotenvy::dotenv().ok();
    let mut config = Config::from_env().unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .min_connections(1)
        .connect(&config.database_url)
        .await
        .unwrap();
    run_database_migrations(&pool).await.unwrap();

    let custom_admin_id = Uuid::new_v4();
    config.default_admin_user_id = custom_admin_id;

    let repo = GameRepositoryImpl::new(pool.clone());
    let seeder = Seeder::new(pool.clone(), config.clone());

    let temp_root = std::env::temp_dir().join(format!("meme_test_custom_admin_{}", Uuid::new_v4()));
    let situations_dir = temp_root.join("official").join("situations");
    fs::create_dir_all(&situations_dir).unwrap();

    let pack_id = Uuid::new_v4();
    let config_json = format!(r#"{{
        "pack_id": "{}",
        "name": "Custom Admin Pack",
        "language_code": "en",
        "items": ["Custom Item 1"]
    }}"#, pack_id);
    fs::write(situations_dir.join("en.json"), config_json).unwrap();

    // Run sync with custom admin user ID
    seeder.sync_all(&temp_root).await.unwrap();

    // 1. Verify custom admin user was created in users table with role 'admin'
    #[derive(sqlx::FromRow)]
    struct UserRow {
        id: Uuid,
        role: String,
    }
    let admin_user = sqlx::query_as::<_, UserRow>(
        "SELECT id, role::text as role FROM users WHERE id = $1"
    )
    .bind(custom_admin_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert_eq!(admin_user.id, custom_admin_id);
    assert_eq!(admin_user.role, "admin");

    // 2. Verify pack author_id points to custom admin
    let pack = repo.find_situation_pack(pack_id).await.unwrap().unwrap();
    assert_eq!(pack.author_id, custom_admin_id);

    let _ = fs::remove_dir_all(temp_root);
}

#[tokio::test]
async fn test_seeder_is_official_flag_on_official_and_custom_packs() {
    dotenvy::dotenv().ok();
    let _ = tracing_subscriber::fmt::try_init();

    let config = Config::from_env().unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .min_connections(1)
        .connect(&config.database_url)
        .await
        .unwrap();
    run_database_migrations(&pool).await.unwrap();

    let repo = GameRepositoryImpl::new(pool.clone());
    let seeder = Seeder::new(pool.clone(), config);

    let temp_root = std::env::temp_dir().join(format!("meme_test_seeds_{}", Uuid::new_v4()));
    let situations_dir = temp_root.join("official").join("situations");
    let memes_dir = temp_root.join("official").join("memes");
    let assets_dir = memes_dir.join("assets");
    fs::create_dir_all(&situations_dir).unwrap();
    fs::create_dir_all(&assets_dir).unwrap();

    let official_sit_pack_id = Uuid::new_v4();
    let official_meme_pack_id = Uuid::new_v4();

    let sit_json = format!(r#"{{
        "pack_id": "{}",
        "name": "Official Situations",
        "language_code": "en",
        "items": ["Situation Official 1"]
    }}"#, official_sit_pack_id);
    fs::write(situations_dir.join("en.json"), sit_json).unwrap();

    fs::write(assets_dir.join("sample.png"), b"sample png data").unwrap();
    let meme_json = format!(r#"{{
        "pack_id": "{}",
        "name": "Official Memes",
        "language_code": "en",
        "items": ["assets/sample.png"]
    }}"#, official_meme_pack_id);
    fs::write(memes_dir.join("en.json"), meme_json).unwrap();

    // 1. Sync official packs via seeder
    seeder.sync_all(&temp_root).await.unwrap();

    // 2. Verify official packs have is_official = true
    let sit_pack = repo.find_situation_pack(official_sit_pack_id).await.unwrap().unwrap();
    assert!(sit_pack.is_official, "Seeded situation pack must have is_official = true");

    let meme_pack = repo.find_meme_pack(official_meme_pack_id).await.unwrap().unwrap();
    assert!(meme_pack.is_official, "Seeded meme pack must have is_official = true");

    // 3. Create regular user packs directly via repository (simulating user creation)
    let user_id = Uuid::from_u128(1); // default admin or test user
    let mut tx = repo.begin().await.unwrap();
    let user_sit_pack_id = repo.insert_situation_pack(
        &mut tx,
        user_id,
        "Custom User Situations",
        None,
        LanguageCode::En,
        ContentSafetyLevel::FamilyFriendly,
        true,
    ).await.unwrap();

    let user_meme_pack_id = repo.insert_meme_pack(
        &mut tx,
        user_id,
        "Custom User Memes",
        None,
        LanguageCode::En,
        ContentSafetyLevel::FamilyFriendly,
        true,
    ).await.unwrap();
    tx.commit().await.unwrap();

    // 4. Verify user packs have is_official = false
    let custom_sit = repo.find_situation_pack(user_sit_pack_id).await.unwrap().unwrap();
    assert!(!custom_sit.is_official, "User-created situation pack must have is_official = false");

    let custom_meme = repo.find_meme_pack(user_meme_pack_id).await.unwrap().unwrap();
    assert!(!custom_meme.is_official, "User-created meme pack must have is_official = false");

    // 5. Verify list query correctly reflects is_official flags
    let all_sits = repo.list_situation_packs(user_id).await.unwrap();
    let official_in_list = all_sits.iter().find(|p| p.id == official_sit_pack_id).unwrap();
    let custom_in_list = all_sits.iter().find(|p| p.id == user_sit_pack_id).unwrap();
    assert!(official_in_list.is_official);
    assert!(!custom_in_list.is_official);

    let all_memes = repo.list_meme_packs(user_id).await.unwrap();
    let official_meme_in_list = all_memes.iter().find(|p| p.id == official_meme_pack_id).unwrap();
    let custom_meme_in_list = all_memes.iter().find(|p| p.id == user_meme_pack_id).unwrap();
    assert!(official_meme_in_list.is_official);
    assert!(!custom_meme_in_list.is_official);

    let _ = fs::remove_dir_all(temp_root);
}



