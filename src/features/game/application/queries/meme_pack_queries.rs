use std::sync::Arc;
use uuid::Uuid;

use crate::{
    common::http::error::AppError,
    features::game::{
        application::ports::game_media_manager::GameMediaManager,
        domain::{
            model::{MemePack, PackMemeDetails},
            ports::GameRepository,
        },
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
    media_manager: Arc<dyn GameMediaManager>,
}

impl GetMemePackQuery {
    pub fn new(repo: Arc<dyn GameRepository>, media_manager: Arc<dyn GameMediaManager>) -> Self {
        Self {
            repo,
            media_manager,
        }
    }

    pub async fn execute(
        &self,
        pack_id: Uuid,
        requestor_id: Uuid,
    ) -> Result<MemePackQueryResult, AppError> {
        let pack = self
            .repo
            .find_meme_pack(pack_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Meme pack not found".to_string()))?;

        if !pack.is_public && pack.author_id != requestor_id {
            return Err(AppError::Forbidden(
                "Only pack author can view private pack".to_string(),
            ));
        }

        let pack_memes = self.repo.get_pack_memes_list(pack_id).await?;
        let mut memes = Vec::new();
        for pm in pack_memes {
            let media_url = if let Some(media_id) = pm.media_id {
                self.media_manager
                    .resolve_url(media_id)
                    .await?
                    .unwrap_or_default()
            } else {
                "".to_string()
            };
            memes.push(PackMemeDetails {
                id: pm.id,
                pack_id: pm.pack_id,
                media_id: pm.media_id,
                media_url,
            });
        }

        Ok(MemePackQueryResult { pack, memes })
    }
}

pub struct ListUserMemePacksQuery {
    repo: Arc<dyn GameRepository>,
    media_manager: Arc<dyn GameMediaManager>,
}

impl ListUserMemePacksQuery {
    pub fn new(repo: Arc<dyn GameRepository>, media_manager: Arc<dyn GameMediaManager>) -> Self {
        Self {
            repo,
            media_manager,
        }
    }

    pub async fn execute(&self, author_id: Uuid) -> Result<Vec<MemePackQueryResult>, AppError> {
        let packs = self.repo.list_user_meme_packs(author_id).await?;
        let mut results = Vec::new();
        for pack in packs {
            let pack_memes = self.repo.get_pack_memes_list(pack.id).await?;
            let mut memes = Vec::new();
            for pm in pack_memes {
                let media_url = if let Some(media_id) = pm.media_id {
                    self.media_manager
                        .resolve_url(media_id)
                        .await?
                        .unwrap_or_default()
                } else {
                    "".to_string()
                };
                memes.push(PackMemeDetails {
                    id: pm.id,
                    pack_id: pm.pack_id,
                    media_id: pm.media_id,
                    media_url,
                });
            }
            results.push(MemePackQueryResult { pack, memes });
        }
        Ok(results)
    }
}
