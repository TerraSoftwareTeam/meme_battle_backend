use std::sync::Arc;
use uuid::Uuid;

use crate::{
    common::http::error::AppError,
    features::game::domain::{
        model::{MemePack, PackMemeDetails},
        ports::GameRepository,
    },
};

pub struct MemePackQueryResult {
    pub pack: MemePack,
    pub memes: Vec<PackMemeDetails>,
}

pub struct ListMemePacksQuery {
    repo: Arc<dyn GameRepository>,
}

impl ListMemePacksQuery {
    pub fn new(repo: Arc<dyn GameRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, author_id: Uuid) -> Result<Vec<MemePack>, AppError> {
        self.repo.list_meme_packs(author_id).await
    }
}

pub struct GetMemePackQuery {
    repo: Arc<dyn GameRepository>,
}

impl GetMemePackQuery {
    pub fn new(repo: Arc<dyn GameRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, pack_id: Uuid, requestor_id: Uuid) -> Result<MemePackQueryResult, AppError> {
        let pack = self.repo.find_meme_pack(pack_id).await?
            .ok_or_else(|| AppError::NotFound("Meme pack not found".to_string()))?;

        if !pack.is_public && pack.author_id != requestor_id {
            return Err(AppError::Forbidden("Only pack author can view private pack".to_string()));
        }

        let memes = self.repo.get_pack_memes_list(pack_id).await?;

        Ok(MemePackQueryResult { pack, memes })
    }
}
