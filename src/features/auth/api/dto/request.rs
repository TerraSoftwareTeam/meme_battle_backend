use serde::Deserialize;
use utoipa::ToSchema;
use validator::Validate;

use crate::features::auth::{LoginUser, RefreshSession, RegisterUser};

#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct RegisterAuthUserDto {
    #[validate(length(max = 64, message = "Username cannot exceed 64 characters"))]
    pub username: String,
    #[validate(length(max = 64, message = "Handle cannot exceed 64 characters"))]
    pub handle: String,
    pub password: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct AuthUserDto {
    #[validate(length(max = 64, message = "Handle cannot exceed 64 characters"))]
    pub handle: String,
    pub password: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct RefreshSessionDto {
    #[validate(length(min = 1, message = "Refresh token cannot be empty"))]
    pub refresh_token: String,
}

impl From<RegisterAuthUserDto> for RegisterUser {
    fn from(dto: RegisterAuthUserDto) -> Self {
        Self {
            username: dto.username,
            handle: dto.handle,
            password: dto.password,
        }
    }
}

impl From<AuthUserDto> for LoginUser {
    fn from(dto: AuthUserDto) -> Self {
        Self {
            handle: dto.handle,
            password: dto.password,
        }
    }
}

impl From<RefreshSessionDto> for RefreshSession {
    fn from(dto: RefreshSessionDto) -> Self {
        Self {
            refresh_token: dto.refresh_token,
        }
    }
}
