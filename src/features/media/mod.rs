mod api;
mod application;
mod domain;
mod infra;

pub use api::routes::{media_routes, MediaApiDoc};
pub use application::commands::{
    delete_media::DeleteMediaCommand, mark_media_attached::MarkMediaAttachedCommand,
    upload_media::UploadMediaCommand, upload_media_from_url::UploadMediaFromUrlCommand,
};
pub use application::queries::{
    get_media_asset_url::GetMediaAssetUrlQuery, get_media_by_id::GetMediaByIdQuery,
    get_user_media::GetUserMediaQuery,
};
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
