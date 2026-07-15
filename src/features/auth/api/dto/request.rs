use serde::Deserialize;
use utoipa::ToSchema;
use validator::Validate;

use crate::features::auth::{LoginUser, RefreshSession, RegisterUser, GuestAuth};

#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct RegisterAuthUserDto {
    #[validate(length(max = 64, message = "Username cannot exceed 64 characters"))]
    pub username: String,
    pub password: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct AuthUserDto {
    #[validate(length(max = 64, message = "Username cannot exceed 64 characters"))]
    pub username: String,
    pub password: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct GuestAuthDto {
    #[serde(default)]
    #[schema(nullable, example = json!(null))]
    #[validate(length(max = 64, message = "Username cannot exceed 64 characters"))]
    pub username: Option<String>,
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
            password: dto.password,
        }
    }
}

impl From<AuthUserDto> for LoginUser {
    fn from(dto: AuthUserDto) -> Self {
        Self {
            username: dto.username,
            password: dto.password,
        }
    }
}

impl From<GuestAuthDto> for GuestAuth {
    fn from(dto: GuestAuthDto) -> Self {
        Self {
            username: dto.username,
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

#[derive(Debug, Deserialize, ToSchema, Validate)]
pub struct ChangePasswordDto {
    #[validate(length(min = 6, max = 128, message = "Password must be between 6 and 128 characters"))]
    pub new_password: String,
}
