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

use crate::{command::Batch, command::Command, Error, UnparsedArgs, Verb};
use tokio::io::AsyncReadExt;
use tokio_stream::StreamExt;
use vsmtp_common::Reply;

/// max size of a received command, including addition from all the following extensions:
/// (note: the base size is at 80 characters)
/// - AUTH (+500 characters)
/// - SMTPUTF8 (+10 characters)
const MAX_LINE_SIZE: usize = 1024;

fn find(bytes: &[u8], search: &[u8]) -> Option<usize> {
    bytes
        .windows(search.len())
        .position(|window| window == search)
}

#[allow(clippy::expect_used)]
fn parse_command_line(line: &Vec<u8>) -> Result<Command<Verb, UnparsedArgs>, Error> {
    // TODO: put max len as a parameter

    if line.len() >= MAX_LINE_SIZE {
        return Err(Error::buffer_too_long(MAX_LINE_SIZE, line.len()));
    }
    if find(line, b"\r\n").is_none() {
        return Err(Error::no_crlf());
    }
    Ok(<Verb as strum::VariantNames>::VARIANTS
        .iter()
        .find(|i| line.len() >= i.len() && line[..i.len()].eq_ignore_ascii_case(i.as_bytes()))
        .map_or_else(
            || (Verb::Unknown, UnparsedArgs(line.clone())),
            |verb| {
                (
                    verb.parse().expect("verb found above"),
                    UnparsedArgs(line[verb.len()..].to_vec()),
                )
            },
        ))
}

/// Reader for TCP window
/// it is used only for the internal reader logic and is not exposed to external.
struct ReaderWindow<'win, R: tokio::io::AsyncRead + Unpin + Send> {
    inner: &'win mut R,
    buffer: &'win mut bytes::BytesMut,
    additional_reserve: usize,
    n: usize,
}

impl<'win, R> ReaderWindow<'win, R>
where
    R: tokio::io::AsyncRead + Unpin + Send,
{
    /// return the full read tcp window (~= buffer)
    fn flush_window(
        &'win mut self,
    ) -> impl tokio_stream::Stream<Item = std::io::Result<Vec<u8>>> + 'win {
        async_stream::try_stream! {
            if !self.buffer.is_empty() {
                self.n = self.buffer.len();
            }
            loop {
                if let Some(pos) = find(&self.buffer[..self.n], b"\r\n") {
                    let out = self.buffer.split_to(pos + 2);
                    self.n -= out.len();
                    yield Vec::<u8>::from(out);
                    if self.buffer.is_empty() {
                        return;
                    }
                } else {
                    self.buffer.reserve(self.additional_reserve);
                    let read_size = self.inner.read_buf(self.buffer).await?;
                    if read_size == 0 {
                        return;
                    }
                    self.n += read_size;
                }
            }
        }
    }
}

/// Stream for reading commands from the client.
pub struct Reader<R: tokio::io::AsyncRead + Unpin + Send> {
    inner: R,
    additional_reserve: usize,
    buffer: bytes::BytesMut,
    pipelining_enabled: bool,
}

impl<R: tokio::io::AsyncRead + Unpin + Send> Reader<R> {
    /// Create a new stream.
    #[must_use]
    #[inline]
    pub fn new(tcp_stream: R, enable_pipelining: bool) -> Self {
        Self {
            inner: tcp_stream,
            additional_reserve: 100,
            buffer: bytes::BytesMut::with_capacity(80),
            pipelining_enabled: enable_pipelining,
        }
    }

    /// Consume the instance and return the underlying reader.
    #[must_use]
    #[inline]
    #[allow(clippy::missing_const_for_fn)]
    pub fn into_inner(self) -> R {
        self.inner
    }

    // instantiate a new ReaderWindow object from an existing reader
    #[allow(clippy::wrong_self_convention)]
    fn to_window_reader(&mut self) -> ReaderWindow<'_, R> {
        ReaderWindow {
            inner: &mut self.inner,
            buffer: &mut self.buffer,
            additional_reserve: self.additional_reserve,
            n: 0,
        }
    }

    /// convert a stream to read tcp window.
    /// It returns a batch of command, which is a simple vector of commands
    #[inline]
    pub fn as_window_stream(
        &mut self,
    ) -> impl tokio_stream::Stream<Item = std::io::Result<Batch>> + '_ {
        let pipelined = self.pipelining_enabled; // NOTE: can break with hot-reloading ?
        async_stream::stream! {
            loop {
                let mut batch: Batch = vec![];
                let mut window_reader = self.to_window_reader();

                let window_content = window_reader.flush_window();
                tokio::pin!(window_content);
                while let Some(cmd) = window_content.next().await {
                    batch.push(parse_command_line(&cmd?));
                    if !pipelined {
                        break;
                    }
                }
                yield Ok(batch);
            }
        }
    }

    /// Produce a stream of "\r\n" terminated lines.
    /// Warning: it discard the rest of the line in case of pipelining
    #[inline]
    #[allow(clippy::todo, clippy::missing_panics_doc)]
    pub fn as_line_stream(
        &mut self,
    ) -> impl tokio_stream::Stream<Item = std::io::Result<Vec<u8>>> + '_ {
        async_stream::try_stream! {
            let mut n = 0;

            loop {
                if let Some(pos) = find(&self.buffer[..n], b"\r\n") {
                    let out = self.buffer.split_to(pos + 2);
                    n -= out.len();

                    yield Vec::<u8>::from(out);
                } else {
                    self.buffer.reserve(self.additional_reserve);
                    let read_size = self.inner.read_buf(&mut self.buffer).await?;
                    if read_size == 0 {
                        if !self.buffer.is_empty() {
                            todo!("what about the remaining buffer? {:?}", self.buffer);
                        }
                        return;
                    }
                    n += read_size;
                }
            }
        }
    }

    /// Produce a stream of lines to generate IMF compliant messages.
    #[inline]
    pub fn as_message_stream(
        &mut self,
        size_limit: usize,
    ) -> impl tokio_stream::Stream<Item = Result<Vec<u8>, Error>> + '_ {
        async_stream::stream! {
            let mut size = 0;

            for await line in self.as_line_stream() {
                let mut line = line?;
                tracing::trace!("<< {:?}", std::str::from_utf8(&line));

                if line == b".\r\n" {
                    return;
                }
                if line.first() == Some(&b'.') {
                    line = line[1..].to_vec();
                }

                // TODO: handle line length max ?
                size += line.len();
                if size >= size_limit {
                    yield Err(Error::buffer_too_long(size_limit, size));
                    return;
                }

                yield Ok(line);
            }
        }
    }

    /// Produce a stream of SMTP replies.
    #[inline]
    pub fn as_reply_stream(
        &mut self,
    ) -> impl tokio_stream::Stream<Item = Result<Reply, Error>> + '_ {
        async_stream::stream! {
            let line_stream = self.as_line_stream();
            tokio::pin!(line_stream);

            loop {
                let mut next_reply = Vec::with_capacity(MAX_LINE_SIZE);

                loop {
                    let new_line = line_stream.next().await;
                    let new_line = match new_line {
                        Some(new_line) => new_line?,
                        None => return,
                    };

                    next_reply.extend_from_slice(&new_line);
                    if new_line.get(3) == Some(&b' ') {
                        break;
                    }
                }

                let next_reply = std::str::from_utf8(&next_reply);
                tracing::trace!("<< {:?}", next_reply);
                yield <Reply as std::str::FromStr>::from_str(next_reply?)
                    .map_err(|e| std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        e.to_string()
                    ).into());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use tokio_stream::StreamExt;

    use crate::{
        command::{self, Batch},
        Error,
    };

    #[allow(clippy::unwrap_used)]
    #[tokio::test]
    async fn flush_window_several_lines() {
        let input = [
            "MAIL FROM:<mrose@dbc.mtview.ca.us>\r\n",
            "RCPT TO:<ned@innosoft.com>\r\n",
            "RCPT TO:<dan@innosoft.com>\r\n",
            "RCPT TO:<kvc@innosoft.com>\r\n",
        ]
        .concat();

        let cursor = std::io::Cursor::new(input);
        let mut reader = super::Reader::new(cursor, true);
        let mut window = reader.to_window_reader();

        let output_stream = window.flush_window();
        tokio::pin!(output_stream);

        assert_eq!(
            output_stream.try_next().await.unwrap(),
            Some(b"MAIL FROM:<mrose@dbc.mtview.ca.us>\r\n".to_vec()),
        );
        assert_eq!(
            output_stream.try_next().await.unwrap(),
            Some(b"RCPT TO:<ned@innosoft.com>\r\n".to_vec()),
        );
        assert_eq!(
            output_stream.try_next().await.unwrap(),
            Some(b"RCPT TO:<dan@innosoft.com>\r\n".to_vec()),
        );
        assert_eq!(
            output_stream.try_next().await.unwrap(),
            Some(b"RCPT TO:<kvc@innosoft.com>\r\n".to_vec()),
        );
        assert_eq!(output_stream.try_next().await.unwrap(), None,);
        assert_eq!(output_stream.try_next().await.unwrap(), None,);
    }

    #[allow(clippy::unwrap_used, clippy::restriction)]
    fn assert_cmd_batch(to_evaluate: &Batch, to_compare: &Batch) {
        for (i, cmd) in to_evaluate.iter().enumerate() {
            cmd.as_ref().map_or_else(
                |_| {
                    assert!(to_compare[i].is_err());
                },
                |cmd| {
                    let expected_cmd = to_compare[i].as_ref().unwrap();
                    assert_eq!(cmd, expected_cmd);
                },
            );
        }
    }

    #[allow(clippy::unwrap_used)]
    #[tokio::test]
    async fn window_stream_single_lines() {
        let input = ["MAIL FROM:<mrose@dbc.mtview.ca.us>\r\n"].concat();

        let cursor = std::io::Cursor::new(input);
        let mut reader = super::Reader::new(cursor, true);
        let stream = reader
            .as_window_stream()
            .timeout(std::time::Duration::from_secs(30));
        tokio::pin!(stream);
        let output = stream.try_next().await.unwrap().unwrap().unwrap();
        let expected = vec![std::result::Result::<
            (command::Verb, command::UnparsedArgs),
            Error,
        >::Ok((
            command::Verb::MailFrom,
            command::UnparsedArgs(b"<mrose@dbc.mtview.ca.us>\r\n".to_vec()),
        ))];
        assert_cmd_batch(&output, &expected);
    }

    #[allow(clippy::unwrap_used)]
    #[tokio::test]
    async fn window_stream_multiple_lines() {
        let input = [
            "MAIL FROM:<mrose@dbc.mtview.ca.us>\r\n",
            "RCPT TO:<ned@innosoft.com>\r\n",
            "RCPT TO:<dan@innosoft.com>\r\n",
            "RCPT TO:<kvc@innosoft.com>\r\n",
        ]
        .concat();

        let cursor = std::io::Cursor::new(input);
        let mut reader = super::Reader::new(cursor, true);
        let stream = reader
            .as_window_stream()
            .timeout(std::time::Duration::from_secs(30));
        tokio::pin!(stream);
        let output = stream.try_next().await.unwrap().unwrap().unwrap();
        let expected = vec![
            std::result::Result::<(command::Verb, command::UnparsedArgs), Error>::Ok((
                command::Verb::MailFrom,
                command::UnparsedArgs(b"<mrose@dbc.mtview.ca.us>\r\n".to_vec()),
            )),
            std::result::Result::<(command::Verb, command::UnparsedArgs), Error>::Ok((
                command::Verb::RcptTo,
                command::UnparsedArgs(b"<ned@innosoft.com>\r\n".to_vec()),
            )),
            std::result::Result::<(command::Verb, command::UnparsedArgs), Error>::Ok((
                command::Verb::RcptTo,
                command::UnparsedArgs(b"<dan@innosoft.com>\r\n".to_vec()),
            )),
            std::result::Result::<(command::Verb, command::UnparsedArgs), Error>::Ok((
                command::Verb::RcptTo,
                command::UnparsedArgs(b"<kvc@innosoft.com>\r\n".to_vec()),
            )),
        ];
        assert_cmd_batch(&output, &expected);
    }

    #[allow(clippy::unwrap_used)]
    #[tokio::test]
    async fn window_stream_multiple_lines_remaining() {
        let input = [
            "MAIL FROM:<mrose@dbc.mtview.ca.us>\r\n",
            "RCPT TO:<ned@innosoft.com>\r\n",
            "RCPT TO:<dan@innosoft.com>\r\n",
            "RCPT TO:<kvc@innosoft.com>",
        ]
        .concat();

        let cursor = std::io::Cursor::new(input);
        let mut reader = super::Reader::new(cursor, true);
        let stream = reader
            .as_window_stream()
            .timeout(std::time::Duration::from_secs(30));
        tokio::pin!(stream);
        let output = stream.try_next().await.unwrap().unwrap().unwrap();
        let expected = vec![
            std::result::Result::<(command::Verb, command::UnparsedArgs), Error>::Ok((
                command::Verb::MailFrom,
                command::UnparsedArgs(b"<mrose@dbc.mtview.ca.us>\r\n".to_vec()),
            )),
            std::result::Result::<(command::Verb, command::UnparsedArgs), Error>::Ok((
                command::Verb::RcptTo,
                command::UnparsedArgs(b"<ned@innosoft.com>\r\n".to_vec()),
            )),
            std::result::Result::<(command::Verb, command::UnparsedArgs), Error>::Ok((
                command::Verb::RcptTo,
                command::UnparsedArgs(b"<dan@innosoft.com>\r\n".to_vec()),
            )),
            std::result::Result::<(command::Verb, command::UnparsedArgs), Error>::Err(
                Error::no_crlf(),
            ),
        ];
        assert_cmd_batch(&output, &expected);
    }

    #[allow(clippy::unwrap_used)]
    #[tokio::test]
    async fn window_stream_no_lines() {
        let input: String = String::new();
        let cursor = std::io::Cursor::new(input);
        let mut reader = super::Reader::new(cursor, true);
        let stream = reader
            .as_window_stream()
            .timeout(std::time::Duration::from_secs(30));
        tokio::pin!(stream);
        let output = stream.try_next().await.unwrap().unwrap().unwrap();
        assert!(output.is_empty());
    }
}
