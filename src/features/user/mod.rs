mod api;
mod application;
mod domain;
mod infra;

// Re-export commonly used items for convenience
pub use api::routes::{user_routes, UserAdminApiDoc, UserApiDoc};
pub use application::commands::{
    promote_to_admin::PromoteToAdminCommand, update_me::UpdateMeCommand,
    update_my_avatar::UpdateMyAvatarCommand,
};
pub use application::models::{
    avatar_upload::{AvatarUploadFile, UploadedAvatar},
    user_profile::UserProfile,
};
pub use application::ports::avatar_media_uploader::AvatarMediaUploader;
pub use application::ports::media_asset_resolver::MediaAssetResolver;
pub use application::queries::{
    get_me::GetMeQuery, get_user_by_id::GetUserByIdQuery, get_user_list::GetUserListQuery,
    get_users::GetUsersQuery,
};
pub use domain::{
    model::{SearchUser, UpdateUserProfile, User},
    ports::user_repository::UserRepository,
};
pub use infra::adapters::user_repository_impl::UserRepositoryImpl;
