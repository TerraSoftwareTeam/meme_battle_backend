use std::sync::Arc;

use crate::features::user::{
    GetMeQuery, GetUserByIdQuery, GetUserListQuery, GetUsersQuery, PromoteToAdminCommand,
    UpdateMeCommand,
};

#[derive(Clone)]
pub struct UserState {
    pub update_me: Arc<UpdateMeCommand>,
    pub get_me: Arc<GetMeQuery>,
    pub get_user_by_id: Arc<GetUserByIdQuery>,
    pub get_user_list: Arc<GetUserListQuery>,
    pub get_users: Arc<GetUsersQuery>,
    pub promote_to_admin: Arc<PromoteToAdminCommand>,
    pub max_file_size_bytes: usize,
}

impl UserState {
    pub fn new(
        update_me: Arc<UpdateMeCommand>,
        get_me: Arc<GetMeQuery>,
        get_user_by_id: Arc<GetUserByIdQuery>,
        get_user_list: Arc<GetUserListQuery>,
        get_users: Arc<GetUsersQuery>,
        promote_to_admin: Arc<PromoteToAdminCommand>,
        max_file_size_bytes: usize,
    ) -> Self {
        Self {
            update_me,
            get_me,
            get_user_by_id,
            get_user_list,
            get_users,
            promote_to_admin,
            max_file_size_bytes,
        }
    }
}
