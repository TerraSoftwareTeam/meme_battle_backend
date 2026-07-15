use std::sync::Arc;

use crate::features::auth::{
    AuthAsGuestCommand, ChangePasswordCommand, LoginUserCommand, RefreshSessionCommand,
    RegisterUserCommand, UserExistsQuery,
};

#[derive(Clone)]
pub struct AuthState {
    pub register_user: Arc<RegisterUserCommand>,
    pub login_user: Arc<LoginUserCommand>,
    pub auth_as_guest: Arc<AuthAsGuestCommand>,
    pub refresh_session: Arc<RefreshSessionCommand>,
    pub user_exists: Arc<UserExistsQuery>,
    pub change_password: Arc<ChangePasswordCommand>,
}

impl AuthState {
    pub fn new(
        register_user: Arc<RegisterUserCommand>,
        login_user: Arc<LoginUserCommand>,
        auth_as_guest: Arc<AuthAsGuestCommand>,
        refresh_session: Arc<RefreshSessionCommand>,
        user_exists: Arc<UserExistsQuery>,
        change_password: Arc<ChangePasswordCommand>,
    ) -> Self {
        Self {
            register_user,
            login_user,
            auth_as_guest,
            refresh_session,
            user_exists,
            change_password,
        }
    }
}
