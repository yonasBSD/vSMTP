//! vSMTP server

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

#![doc(html_no_source)]
#![deny(missing_docs)]
#![deny(unsafe_code)]
//
#![warn(rust_2018_idioms)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::cargo)]
//
#![allow(clippy::significant_drop_tightening)]

mod channel_message;
mod runtime;
mod server;
mod receiver {
    pub mod handler;
    mod post_transaction;
    pub mod pre_transaction;
}

/// This module is responsible of the delivery of the message, and the management of failures.
pub mod delivery;
/// This module is responsible of the communication between the different part of the software.
pub mod scheduler;
/// This module execute logics on message after taking their responsibility, and before sending them.
pub mod working;

pub use channel_message::ProcessMessage;
pub use receiver::handler::Handler;
pub use receiver::pre_transaction::ValidationVSL;
pub use runtime::start_runtime;
pub use server::{socket_bind_anyhow, Server};

use anyhow::Context;
use vsmtp_common::status::SmtpConnection;
use vsmtp_common::{Address, ContextFinished};
use vsmtp_mail_parser::MessageBody;

/// delegate a message to another service.
pub(crate) fn delegate(
    delegator: &SmtpConnection,
    context: &ContextFinished,
    message: &MessageBody,
) -> anyhow::Result<lettre::transport::smtp::response::Response> {
    use lettre::Transport;

    let envelope = lettre::address::Envelope::new(
        context
            .mail_from
            .reverse_path
            .as_ref()
            .map(Address::to_lettre),
        context
            .rcpt_to
            .delivery
            .values()
            .flatten()
            .map(|rcpt| rcpt.0.to_lettre())
            .collect::<Vec<_>>(),
    )?;

    delegator
        .0
        .lock()
        .unwrap()
        .send_raw(&envelope, message.inner().to_string().as_bytes())
        .context("failed to delegate email")
}
