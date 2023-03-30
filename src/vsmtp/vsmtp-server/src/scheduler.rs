/*
 * vSMTP mail transfer agent
 * Copyright (C) 2023 viridIT SAS
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

use crate::ProcessMessage;

/// This instance can emit message to the different part of the software.
pub struct Emitter {
    working: tokio::sync::mpsc::Sender<ProcessMessage>,
    delivery: tokio::sync::mpsc::Sender<ProcessMessage>,
}

impl Emitter {
    #[tracing::instrument(skip(self))]
    pub(crate) async fn send_to_delivery(&self, message: ProcessMessage) -> std::io::Result<()> {
        match self.delivery.send(message).await {
            Ok(()) => Ok(()),
            Err(_err) => Err(std::io::Error::from(std::io::ErrorKind::ConnectionAborted)),
        }
    }

    #[tracing::instrument(skip(self))]
    pub(crate) async fn send_to_working(&self, message: ProcessMessage) -> std::io::Result<()> {
        match self.working.send(message).await {
            Ok(()) => Ok(()),
            Err(_err) => Err(std::io::Error::from(std::io::ErrorKind::ConnectionAborted)),
        }
    }
}

/// This instance can receive message from the different part of the software.
pub struct Receiver {
    inner: tokio::sync::mpsc::Receiver<ProcessMessage>,
}

impl Receiver {
    /// Produce a stream of message.
    pub fn as_stream(&mut self) -> impl tokio_stream::Stream<Item = ProcessMessage> + '_ {
        async_stream::stream! {
            while let Some(message) = self.inner.recv().await {
                yield message;
            }
        }
    }
}

/// This instance is responsible of the communication between the different part of the software.
///
/// **receiver**  <->  **working**  <->  **delivery**
#[must_use]
pub fn init(
    working_channel_size: usize,
    delivery_channel_size: usize,
) -> (std::sync::Arc<Emitter>, Receiver, Receiver) {
    let (working_tx, working_rx) = tokio::sync::mpsc::channel(working_channel_size);
    let (delivery_tx, delivery_rx) = tokio::sync::mpsc::channel(delivery_channel_size);

    (
        std::sync::Arc::new(Emitter {
            working: working_tx,
            delivery: delivery_tx,
        }),
        Receiver { inner: working_rx },
        Receiver { inner: delivery_rx },
    )
}
