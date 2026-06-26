use std::sync::Arc;

use crate::features::auth::{
    AuthAsGuestCommand, LoginUserCommand, RefreshSessionCommand, RegisterUserCommand,
};

#[derive(Clone)]
pub struct AuthState {
    pub register_user: Arc<RegisterUserCommand>,
    pub login_user: Arc<LoginUserCommand>,
    pub auth_as_guest: Arc<AuthAsGuestCommand>,
    pub refresh_session: Arc<RefreshSessionCommand>,
}

impl AuthState {
    pub fn new(
        register_user: Arc<RegisterUserCommand>,
        login_user: Arc<LoginUserCommand>,
        auth_as_guest: Arc<AuthAsGuestCommand>,
        refresh_session: Arc<RefreshSessionCommand>,
    ) -> Self {
        Self {
            register_user,
            login_user,
            auth_as_guest,
            refresh_session,
        }
    }
}
