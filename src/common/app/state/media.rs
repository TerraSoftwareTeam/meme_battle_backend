use std::sync::Arc;

use crate::features::media::{
    DeleteMediaCommand, GetMediaAssetUrlQuery, GetMediaByIdQuery, GetUserMediaQuery,
    MarkMediaAttachedCommand, UploadMediaCommand, UploadMediaFromUrlCommand,
};

#[derive(Clone)]
pub struct MediaState {
    pub upload_media: Arc<UploadMediaCommand>,
    pub upload_media_from_url: Arc<UploadMediaFromUrlCommand>,
    pub delete_media: Arc<DeleteMediaCommand>,
    pub get_media_by_id: Arc<GetMediaByIdQuery>,
    pub get_user_media: Arc<GetUserMediaQuery>,
    pub get_media_asset_url: Arc<GetMediaAssetUrlQuery>,
    pub mark_media_attached: Arc<MarkMediaAttachedCommand>,
}

impl MediaState {
    pub fn new(
        upload_media: Arc<UploadMediaCommand>,
        upload_media_from_url: Arc<UploadMediaFromUrlCommand>,
        delete_media: Arc<DeleteMediaCommand>,
        get_media_by_id: Arc<GetMediaByIdQuery>,
        get_user_media: Arc<GetUserMediaQuery>,
        get_media_asset_url: Arc<GetMediaAssetUrlQuery>,
        mark_media_attached: Arc<MarkMediaAttachedCommand>,
    ) -> Self {
        Self {
            upload_media,
            upload_media_from_url,
            delete_media,
            get_media_by_id,
            get_user_media,
            get_media_asset_url,
            mark_media_attached,
        }
    }
}
