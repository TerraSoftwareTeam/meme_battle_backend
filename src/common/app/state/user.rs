use std::sync::Arc;

use crate::features::user::{
    GetMeQuery, GetUserByIdQuery, GetUserListQuery, GetUsersQuery, PromoteToAdminCommand,
    UpdateMeCommand, UpdateMyAvatarCommand,
};

#[derive(Clone)]
pub struct UserState {
    pub update_me: Arc<UpdateMeCommand>,
    pub update_my_avatar: Arc<UpdateMyAvatarCommand>,
    pub get_me: Arc<GetMeQuery>,
    pub get_user_by_id: Arc<GetUserByIdQuery>,
    pub get_user_list: Arc<GetUserListQuery>,
    pub get_users: Arc<GetUsersQuery>,
    pub promote_to_admin: Arc<PromoteToAdminCommand>,
}

impl UserState {
    pub fn new(
        update_me: Arc<UpdateMeCommand>,
        update_my_avatar: Arc<UpdateMyAvatarCommand>,
        get_me: Arc<GetMeQuery>,
        get_user_by_id: Arc<GetUserByIdQuery>,
        get_user_list: Arc<GetUserListQuery>,
        get_users: Arc<GetUsersQuery>,
        promote_to_admin: Arc<PromoteToAdminCommand>,
    ) -> Self {
        Self {
            update_me,
            update_my_avatar,
            get_me,
            get_user_by_id,
            get_user_list,
            get_users,
            promote_to_admin,
        }
    }
}
