use std::sync::Arc;
use uuid::Uuid;

use crate::{
    common::http::error::AppError,
    features::game::domain::{
        model::{SituationPack, PackSituation},
        ports::GameRepository,
    },
};

pub struct SituationPackQueryResult {
    pub pack: SituationPack,
    pub situations: Vec<PackSituation>,
}

pub struct ListSituationPacksQuery {
    repo: Arc<dyn GameRepository>,
}

impl ListSituationPacksQuery {
    pub fn new(repo: Arc<dyn GameRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, author_id: Uuid) -> Result<Vec<SituationPack>, AppError> {
        self.repo.list_situation_packs(author_id).await
    }
}

pub struct GetSituationPackQuery {
    repo: Arc<dyn GameRepository>,
}

impl GetSituationPackQuery {
    pub fn new(repo: Arc<dyn GameRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, pack_id: Uuid, requestor_id: Uuid) -> Result<SituationPackQueryResult, AppError> {
        let pack = self.repo.find_situation_pack(pack_id).await?
            .ok_or_else(|| AppError::NotFound("Situation pack not found".to_string()))?;

        if !pack.is_public && pack.author_id != requestor_id {
            return Err(AppError::Forbidden("Only pack author can view private pack".to_string()));
        }

        let situations = self.repo.get_pack_situations_list(pack_id).await?;

        Ok(SituationPackQueryResult { pack, situations })
    }
}
