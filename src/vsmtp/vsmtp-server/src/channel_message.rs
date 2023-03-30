/*
 * vSMTP mail transfer agent
 * Copyright (C) 2022 viridIT SAS
 *
 * This program is free software: you can redistribute it and/or modify it under
 * the terms of the GNU General Public License as published by the Free Software
 * Foundation, either version 3 of the License, or any later version.
 *
 * This program is distributed in the hope that it will be useful, but WITHOUT
 * ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
 * FOR A PARTICULAR PURPOSE.  See the GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License along with
 * this program. If not, see https://www.gnu.org/licenses/.
 *
*/

/// Payload sent across the different processes of `vSMTP`.
#[must_use]
#[derive(Debug)]
pub struct ProcessMessage {
    message_uuid: uuid::Uuid,
    /// is the email stored in the delegated queue.
    delegated: bool,
}

impl ProcessMessage {
    /// Construct a new `ProcessMessage`.
    pub const fn new(message_uuid: uuid::Uuid) -> Self {
        Self {
            message_uuid,
            delegated: false,
        }
    }

    pub(crate) const fn delegated(message_uuid: uuid::Uuid) -> Self {
        Self {
            message_uuid,
            delegated: true,
        }
    }

    pub(crate) const fn is_from_delegation(&self) -> bool {
        self.delegated
    }
}

impl AsRef<uuid::Uuid> for ProcessMessage {
    fn as_ref(&self) -> &uuid::Uuid {
        &self.message_uuid
    }
}

#[cfg(test)]
mod test {
    use crate::ProcessMessage;

    #[test]
    fn debug() {
        println!(
            "{:?}",
            ProcessMessage {
                message_uuid: uuid::Uuid::nil(),
                delegated: false,
            }
        );
    }
}
