#[derive(Debug, Clone)]
pub struct AvatarUploadFile {
    pub filename: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct UploadedAvatar {
    pub media_asset_id: i64,
    pub url: String,
}
