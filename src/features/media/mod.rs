mod api;
mod application;
mod domain;
mod infra;

pub use api::routes::{media_routes, MediaApiDoc};
pub use application::commands::{
    mark_media_attached::MarkMediaAttachedCommand, upload_media::UploadMediaCommand,
};
pub use application::queries::get_media_asset_url::GetMediaAssetUrlQuery;
pub use domain::{
    model::{MediaAsset, MediaProvider, MediaStatus, MediaVisibility, StoredFile, UploadFile},
    ports::{
        file_storage::FileStorage,
        media_repository::{CreateMediaAsset, MediaRepository},
    },
};
pub use infra::adapters::{
    hackclub_cdn_storage::HackClubCdnStorage, postgres_media_repository::PostgresMediaRepository,
};
