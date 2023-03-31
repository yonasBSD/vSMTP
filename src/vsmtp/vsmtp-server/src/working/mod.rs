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
use crate::{
    delegate,
    scheduler::{self, Emitter},
    ProcessMessage,
};
use anyhow::Context;
use tokio_stream::StreamExt;
use vqueue::{GenericQueueManager, QueueID};
use vsmtp_common::{
    status,
    transfer::{self, error::Rule},
};
use vsmtp_rule_engine::{ExecutionStage, RuleEngine};

pub(super) async fn start<Q: GenericQueueManager + Sized + 'static>(
    rule_engine: std::sync::Arc<RuleEngine>,
    queue_manager: std::sync::Arc<Q>,
    emitter: std::sync::Arc<Emitter>,
    mut receiver: scheduler::Receiver,
) {
    let working_receiver = receiver.as_stream().map(|pm| {
        tokio::spawn(handle_one(
            rule_engine.clone(),
            queue_manager.clone(),
            pm,
            emitter.clone(),
        ))
    });
    tokio::pin!(working_receiver);

    while let Some(_join_handle) = working_receiver.next().await {}
}

/// Handle one message in the working queue.
///
/// Running the rule engine at the stage `PostQ` and then
/// handle the quarantine, delegation or delivery outcome of the message.
#[allow(clippy::too_many_lines)]
#[tracing::instrument(name = "working", skip_all, err)]
pub async fn handle_one<Q: GenericQueueManager + Sized + 'static>(
    rule_engine: std::sync::Arc<RuleEngine>,
    queue_manager: std::sync::Arc<Q>,
    process_message: ProcessMessage,
    emitter: std::sync::Arc<Emitter>,
) -> anyhow::Result<()> {
    struct Opt {
        move_to_queue: Option<QueueID>,
        send_to_delivery: bool,
        write_email: bool,
        delegated: bool,
    }

    let queue = if process_message.is_from_delegation() {
        QueueID::Delegated
    } else {
        QueueID::Working
    };

    let (ctx, mail_message) = queue_manager
        .get_both(&queue, process_message.as_ref())
        .await?;

    let mut skipped = ctx.connect.skipped.clone();
    let (ctx, mail_message, _) = rule_engine.just_run_when(
        &mut skipped,
        ExecutionStage::PostQ,
        vsmtp_common::Context::Finished(ctx),
        mail_message,
    );

    let mut ctx = ctx.unwrap_finished().context("context is not finished")?;

    let Opt {
        move_to_queue,
        send_to_delivery,
        write_email,
        delegated,
    } = match &skipped {
        Some(status::Status::Quarantine(path)) => {
            queue_manager
                .move_to(&queue, &QueueID::Quarantine { name: path.into() }, &ctx)
                .await?;

            tracing::warn!(stage = %ExecutionStage::PostQ, status = "quarantine", "Rules skipped.");
            Opt {
                move_to_queue: None,
                send_to_delivery: false,
                write_email: true,
                delegated: false,
            }
        }
        Some(status @ status::Status::Delegated(delegator)) => {
            ctx.connect.skipped = Some(status::Status::DelegationResult);

            // NOTE:  moving here because the delegation process could try to
            //        pickup the email before it's written on disk.
            queue_manager
                .clone()
                .move_to(&queue, &QueueID::Delegated, &ctx)
                .await?;

            queue_manager
                .write_msg(process_message.as_ref(), &mail_message)
                .await?;

            // NOTE: needs to be executed after writing, because the other
            //       thread could pickup the email faster than this function.
            delegate(delegator, &ctx, &mail_message)?;

            tracing::warn!(stage = %ExecutionStage::PostQ, status = status.as_ref(), "Rules skipped.");

            Opt {
                move_to_queue: None,
                send_to_delivery: false,
                write_email: false,
                delegated: true,
            }
        }
        Some(status::Status::DelegationResult) => Opt {
            move_to_queue: None,
            send_to_delivery: true,
            write_email: true,
            delegated: true,
        },
        Some(status::Status::Deny(code)) => {
            for rcpt in &mut ctx.rcpt_to.delivery.values_mut().flatten() {
                rcpt.1 = transfer::Status::failed(Rule::Denied(code.clone()));
            }

            Opt {
                move_to_queue: Some(QueueID::Dead),
                send_to_delivery: false,
                write_email: true,
                delegated: false,
            }
        }
        None | Some(status::Status::Next) => Opt {
            move_to_queue: Some(QueueID::Deliver),
            send_to_delivery: true,
            write_email: true,
            delegated: false,
        },
        Some(reason) => {
            tracing::warn!(status = ?reason, "Rules skipped.");
            Opt {
                move_to_queue: Some(QueueID::Deliver),
                send_to_delivery: true,
                write_email: true,
                delegated: false,
            }
        }
    };

    if write_email {
        queue_manager
            .write_msg(process_message.as_ref(), &mail_message)
            .await?;
    }

    if let Some(next_queue) = move_to_queue {
        queue_manager.move_to(&queue, &next_queue, &ctx).await?;
    }

    if send_to_delivery {
        emitter
            .send_to_delivery(if delegated {
                ProcessMessage::delegated
            } else {
                ProcessMessage::new
            }(*process_message.as_ref()))
            .await?;
    }

    Ok(())
}
