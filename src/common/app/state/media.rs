use std::sync::Arc;

use crate::features::media::{
    GetMediaAssetUrlQuery, MarkMediaAttachedCommand, UploadMediaCommand,
};

#[derive(Clone)]
pub struct MediaState {
    pub upload_media: Arc<UploadMediaCommand>,
    pub get_media_asset_url: Arc<GetMediaAssetUrlQuery>,
    pub mark_media_attached: Arc<MarkMediaAttachedCommand>,
}

impl MediaState {
    pub fn new(
        upload_media: Arc<UploadMediaCommand>,
        get_media_asset_url: Arc<GetMediaAssetUrlQuery>,
        mark_media_attached: Arc<MarkMediaAttachedCommand>,
    ) -> Self {
        Self {
            upload_media,
            get_media_asset_url,
            mark_media_attached,
        }
    }
}
