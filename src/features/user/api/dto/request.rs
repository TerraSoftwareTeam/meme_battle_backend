use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

use crate::features::user::{SearchUser, UpdateMeInput};

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SearchUserDto {
    pub id: Option<String>,
    pub username: Option<String>,
}

impl From<SearchUserDto> for SearchUser {
    fn from(dto: SearchUserDto) -> Self {
        Self {
            id: dto.id,
            username: dto.username,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, Validate)]
pub struct UpdateMeDto {
    #[validate(length(min = 1, max = 64, message = "Username must be between 1 and 64 characters"))]
    pub username: Option<String>,
    #[validate(length(min = 1, max = 128, message = "Password cannot be empty"))]
    pub password: Option<String>,
}

impl From<UpdateMeDto> for UpdateMeInput {
    fn from(dto: UpdateMeDto) -> Self {
        Self {
            username: dto.username,
            password: dto.password,
        }
    }
}
