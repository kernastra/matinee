//! A signed-in Matinee user.
//!
//! This is not a credential. The access token stays in the client session.

use crate::id::UserId;
use crate::images::ImageTag;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct User {
    pub id: UserId,
    pub name: String,
    pub avatar: Option<ImageTag>,
}

impl User {
    pub fn new(id: UserId, name: impl Into<String>, avatar: Option<ImageTag>) -> Self {
        Self {
            id,
            name: name.into(),
            avatar,
        }
    }

    pub fn id(&self) -> &UserId {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn has_avatar(&self) -> bool {
        self.avatar.is_some()
    }
}
