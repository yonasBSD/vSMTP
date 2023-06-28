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

use crate::{ConnectionKind, Error, ParseArgsError};
use vsmtp_common::{auth::Mechanism, Address, ClientName, Domain};

macro_rules! strip_suffix_crlf {
    ($v:expr) => {
        $v.0.strip_suffix(b"\r\n")
            .ok_or(ParseArgsError::InvalidArgs)?
    };
}

fn strip_quote(input: &[u8]) -> Result<&[u8], ParseArgsError> {
    input
        .strip_prefix(b"<")
        .ok_or(ParseArgsError::InvalidArgs)?
        .strip_suffix(b">")
        .ok_or(ParseArgsError::InvalidArgs)
}

/// Buffer received from the client.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct UnparsedArgs(pub Vec<u8>);

pub type Command<Verb, Args> = (Verb, Args);

/// Information received from the client at the connection TCP/IP.
#[non_exhaustive]
pub struct AcceptArgs {
    /// Address of the server which accepted the connection.
    pub client_addr: std::net::SocketAddr,
    /// Peer address of the connection.
    pub server_addr: std::net::SocketAddr,
    /// Instant when the connection was accepted.
    pub timestamp: time::OffsetDateTime,
    /// Universal unique identifier of the connection.
    pub uuid: uuid::Uuid,
    /// Kind of connection.
    pub kind: ConnectionKind,
}

impl AcceptArgs {
    /// Create a new instance.
    #[inline]
    #[must_use]
    pub const fn new(
        client_addr: std::net::SocketAddr,
        server_addr: std::net::SocketAddr,
        timestamp: time::OffsetDateTime,
        uuid: uuid::Uuid,
        kind: ConnectionKind,
    ) -> Self {
        Self {
            client_addr,
            server_addr,
            timestamp,
            uuid,
            kind,
        }
    }
}

/// Information received from the client at the HELO command.
#[non_exhaustive]
pub struct HeloArgs {
    /// Name of the client.
    pub client_name: Domain,
}

/// Information received from the client at the EHLO command.
#[non_exhaustive]
pub struct EhloArgs {
    /// Name of the client.
    pub client_name: ClientName,
}

/// See "SMTP Service Extension for 8-bit MIME Transport"
/// <https://datatracker.ietf.org/doc/html/rfc6152>
#[derive(strum::EnumVariantNames, strum::EnumString)]
pub enum MimeBodyType {
    ///
    #[strum(serialize = "7BIT")]
    SevenBit,
    ///
    #[strum(serialize = "8BITMIME")]
    EightBitMime,
    // TODO: https://datatracker.ietf.org/doc/html/rfc3030
    // Binary,
}

/// <https://www.rfc-editor.org/rfc/rfc3461>
/// return either the full message or only the headers.
/// Only applies to DSNs that indicate delivery failure for at least one recipient.
/// If a DSN contains no indications of delivery failure, only the headers of the message should be returned.
#[allow(clippy::exhaustive_enums)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DsnReturn {
    /// Complete message
    Full,
    /// Only the message headers
    Headers,
}

/// Information received from the client at the MAIL FROM command.
#[non_exhaustive]
pub struct MailFromArgs {
    /// Sender address.
    pub reverse_path: Option<Address>,
    /// (8BITMIME)
    pub mime_body_type: Option<MimeBodyType>,
    // TODO:
    // Option<String>       (AUTH)
    /// (SIZE)
    pub size: Option<usize>,
    /// smtputf8 extension allowing utf8 email
    pub use_smtputf8: bool,
    /// rfc 3461 : client defined identifier for the message. Not the same as the header field `Message-ID` or
    /// `message_uuid`/`connection_uuid` used by vSMTP
    pub envelop_id: Option<String>,
    /// `RET` argument of the `MAIL FROM` command
    pub ret: Option<DsnReturn>,
}

/// <https://www.rfc-editor.org/rfc/rfc3461>
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(clippy::exhaustive_enums)]
pub enum NotifyOn {
    /// This message must explicitly not produce a DSN.
    Never,
    // NOTE: this should be implemented as a bitmask
    /// One or more scenarios that should produce a DSN.
    Some {
        /// The delivery of the message to the recipient was successful.
        success: bool,
        /// The delivery of the message to the recipient failed.
        failure: bool,
        /// The delivery of the message to the recipient has been delayed.
        delay: bool,
    },
}

/// <https://www.rfc-editor.org/rfc/rfc3461>
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[allow(clippy::exhaustive_structs)]
pub struct OriginalRecipient {
    /// The type of address used in the `ORCPT` argument. (rfc822)
    pub addr_type: String,
    /// The original recipient address.
    pub mailbox: Address,
}

/// Information received from the client at the RCPT TO command.
#[non_exhaustive]
pub struct RcptToArgs {
    /// Recipient address.
    pub forward_path: Address,
    /// `ORCPT` argument of the `RCPT TO` command
    pub original_forward_path: Option<OriginalRecipient>,
    /// `NOTIFY` argument of the `RCPT TO` command
    pub notify_on: NotifyOn,
}

/// Information received from the client at the AUTH command.
#[non_exhaustive]
pub struct AuthArgs {
    /// Authentication mechanism.
    pub mechanism: Mechanism,
    /// First buffer of the challenge, optionally issued by the server.
    /// [`base64`] encoded buffer.
    pub initial_response: Option<Vec<u8>>,
}

fn split_args(slice: &[u8]) -> Option<(&[u8], &[u8])> {
    slice.iter().position(|c| *c == b'=').map(|pos| {
        let (k, v) = slice.split_at(pos);
        (k, &v[1..])
    })
}

impl TryFrom<UnparsedArgs> for HeloArgs {
    type Error = ParseArgsError;

    #[inline]
    fn try_from(value: UnparsedArgs) -> Result<Self, Self::Error> {
        let value = strip_suffix_crlf!(value).to_vec();

        Ok(Self {
            client_name: Domain::from_utf8(
                addr::parse_domain_name(&String::from_utf8(value)?)
                    .map_err(|_err| ParseArgsError::InvalidArgs)?
                    .as_str(),
            )
            .map_err(|_err| ParseArgsError::InvalidArgs)?,
        })
    }
}

impl TryFrom<UnparsedArgs> for EhloArgs {
    type Error = ParseArgsError;

    #[inline]
    fn try_from(value: UnparsedArgs) -> Result<Self, Self::Error> {
        let value = String::from_utf8(strip_suffix_crlf!(value).to_vec())?;

        if !value.is_ascii() {
            return Err(ParseArgsError::InvalidArgs);
        }

        let client_name = match &value {
            ipv6 if ipv6.to_lowercase().starts_with("[ipv6:") && ipv6.ends_with(']') => {
                match ipv6.get("[IPv6:".len()..ipv6.len() - 1) {
                    Some(ipv6) => ClientName::Ip6(ipv6.parse::<std::net::Ipv6Addr>()?),
                    None => return Err(ParseArgsError::InvalidArgs),
                }
            }
            ipv4 if ipv4.starts_with('[') && ipv4.ends_with(']') => {
                match ipv4.get(1..ipv4.len() - 1) {
                    Some(ipv4) => ClientName::Ip4(ipv4.parse::<std::net::Ipv4Addr>()?),
                    None => return Err(ParseArgsError::InvalidArgs),
                }
            }
            domain => ClientName::Domain(
                Domain::from_utf8(
                    addr::parse_domain_name(domain)
                        .map_err(|_err| ParseArgsError::InvalidArgs)?
                        .as_str(),
                )
                .map_err(|_err| ParseArgsError::InvalidArgs)?,
            ),
        };

        Ok(Self { client_name })
    }
}

impl TryFrom<UnparsedArgs> for AuthArgs {
    type Error = ParseArgsError;

    #[inline]
    fn try_from(value: UnparsedArgs) -> Result<Self, Self::Error> {
        let value = strip_suffix_crlf!(value);

        let (mechanism, initial_response) = if let Some((idx, _)) = value
            .iter()
            .copied()
            .enumerate()
            .find(|&(_, c)| c.is_ascii_whitespace())
        {
            let (mechanism, initial_response) = value.split_at(idx);
            (
                mechanism.to_vec(),
                Some(
                    initial_response
                        .get(1..)
                        .ok_or(ParseArgsError::InvalidArgs)?
                        .to_vec(),
                ),
            )
        } else {
            (value.to_vec(), None)
        };

        let mechanism = String::from_utf8(mechanism)?
            .parse()
            .map_err(|_err| ParseArgsError::InvalidArgs)?;

        Ok(Self {
            mechanism,
            initial_response,
        })
    }
}

impl MailFromArgs {
    fn parse_arguments(&mut self, raw_args: &[u8]) -> Result<(), ParseArgsError> {
        match split_args(raw_args) {
            #[allow(clippy::expect_used)]
            Some((key, value)) if key.eq_ignore_ascii_case(b"BODY") => {
                if self.mime_body_type.is_some() {
                    Err(ParseArgsError::InvalidArgs)
                } else {
                    self.mime_body_type = <MimeBodyType as strum::VariantNames>::VARIANTS
                        .iter()
                        .find(|i| {
                            value.len() >= i.len()
                                && value
                                    .get(..i.len())
                                    .expect("range checked above")
                                    .eq_ignore_ascii_case(i.as_bytes())
                        })
                        .map(|body| body.parse().expect("body found above"));
                    Ok(())
                }
            }
            Some((key, value)) if key.eq_ignore_ascii_case(b"SIZE") => {
                if self.mime_body_type.is_some() {
                    Err(ParseArgsError::InvalidArgs)
                } else {
                    self.size = Some(
                        std::str::from_utf8(value)?
                            .parse()
                            .map_err(|_e| ParseArgsError::InvalidArgs)?,
                    );
                    Ok(())
                }
            }
            Some((key, value)) if key.eq_ignore_ascii_case(b"RET") => {
                if self.ret.is_some() {
                    Err(ParseArgsError::InvalidArgs)
                } else {
                    self.ret = match value {
                        value if value.eq_ignore_ascii_case(b"FULL") => Some(DsnReturn::Full),
                        value if value.eq_ignore_ascii_case(b"HDRS") => Some(DsnReturn::Headers),
                        _ => return Err(ParseArgsError::InvalidArgs),
                    };
                    Ok(())
                }
            }
            Some((key, value)) if key.eq_ignore_ascii_case(b"ENVID") => {
                if self.envelop_id.is_some() {
                    Err(ParseArgsError::InvalidArgs)
                } else {
                    self.envelop_id = Some(
                        std::str::from_utf8(value)?
                            .parse()
                            .map_err(|_e| ParseArgsError::InvalidArgs)?,
                    );
                    Ok(())
                }
            }
            _ => Err(ParseArgsError::InvalidArgs),
        }
    }

    fn parse_options(&mut self, raw_args: &[u8]) -> Result<(), ParseArgsError> {
        match raw_args {
            b"SMTPUTF8" => {
                self.use_smtputf8 = true;
                Ok(())
            }
            _ => Err(ParseArgsError::InvalidArgs),
        }
    }
}

impl TryFrom<UnparsedArgs> for MailFromArgs {
    type Error = ParseArgsError;

    #[inline]
    fn try_from(value: UnparsedArgs) -> Result<Self, Self::Error> {
        let value = strip_suffix_crlf!(value);

        let mut args = value
            .split(u8::is_ascii_whitespace)
            .filter(|s| !s.is_empty());

        let mailbox = strip_quote(args.next().ok_or(ParseArgsError::InvalidArgs)?)?;
        let mailbox = if mailbox.is_empty() {
            None
        } else {
            Some(String::from_utf8(mailbox.to_vec())?)
        };

        let mut result = Self {
            reverse_path: None,
            mime_body_type: None,
            size: None,
            use_smtputf8: false,
            envelop_id: None,
            ret: None,
        };

        for arg in args {
            if arg.contains(&b'=') {
                result.parse_arguments(arg)?;
            } else {
                result.parse_options(arg)?;
            }
        }

        result.reverse_path = if let Some(mailbox) = mailbox {
            if !result.use_smtputf8 && !mailbox.is_ascii() {
                return Err(ParseArgsError::EmailUnavailable);
            }
            match <Address as std::str::FromStr>::from_str(&mailbox) {
                Ok(mailbox) => Some(mailbox),
                Err(_error) => return Err(ParseArgsError::InvalidMailAddress { mail: mailbox }),
            }
        } else {
            None
        };
        Ok(result)
    }
}

impl RcptToArgs {
    fn parse_arguments(&mut self, raw_args: &[u8]) -> Result<(), ParseArgsError> {
        match split_args(raw_args) {
            #[allow(clippy::expect_used)]
            Some((key, value)) if key.eq_ignore_ascii_case(b"ORCPT") => {
                if self.original_forward_path.is_some() {
                    Err(ParseArgsError::InvalidArgs)
                } else {
                    let (addr_type, addr) = match value.iter().position(|c| *c == b';') {
                        Some(pos) => (&value[..pos], &value[pos + 1..]),
                        None => return Err(ParseArgsError::InvalidArgs),
                    };

                    let value = std::str::from_utf8(addr)?;
                    self.original_forward_path =
                        match <Address as std::str::FromStr>::from_str(value) {
                            Ok(mailbox) => Some(OriginalRecipient {
                                addr_type: std::str::from_utf8(addr_type)?.to_owned(),
                                mailbox,
                            }),
                            Err(_error) => {
                                return Err(ParseArgsError::InvalidMailAddress {
                                    mail: value.to_owned(),
                                })
                            }
                        };
                    Ok(())
                }
            }
            Some((key, value)) if key.eq_ignore_ascii_case(b"NOTIFY") => {
                const SUCCESS: &[u8] = b"SUCCESS";
                const FAILURE: &[u8] = b"FAILURE";
                const DELAY: &[u8] = b"DELAY";
                const VARIANTS: &[&[u8]] = &[SUCCESS, FAILURE, DELAY];

                let mut notify = None;

                let mut begin = 0;
                let it = memchr::memchr_iter(b'|', value);
                for pos in it {
                    let v = &value[begin..=pos];

                    #[allow(clippy::pattern_type_mismatch)]
                    match (v, &mut notify) {
                        (value, Some(NotifyOn::Never))
                            if VARIANTS.iter().any(|i| i.eq_ignore_ascii_case(value)) =>
                        {
                            return Err(ParseArgsError::InvalidArgs)
                        }
                        (value, None) if value.eq_ignore_ascii_case(b"NEVER") => {
                            notify = Some(NotifyOn::Never);
                        }
                        (value, None) if value.eq_ignore_ascii_case(SUCCESS) => {
                            notify = Some(NotifyOn::Some {
                                success: true,
                                failure: false,
                                delay: false,
                            });
                        }
                        (value, None) if value.eq_ignore_ascii_case(b"FAILURE") => {
                            notify = Some(NotifyOn::Some {
                                success: false,
                                failure: true,
                                delay: false,
                            });
                        }
                        (value, None) if value.eq_ignore_ascii_case(DELAY) => {
                            notify = Some(NotifyOn::Some {
                                success: false,
                                failure: false,
                                delay: true,
                            });
                        }
                        (value, Some(NotifyOn::Some { success, .. }))
                            if value.eq_ignore_ascii_case(SUCCESS) =>
                        {
                            *success = true;
                        }
                        (value, Some(NotifyOn::Some { failure, .. }))
                            if value.eq_ignore_ascii_case(FAILURE) =>
                        {
                            *failure = true;
                        }
                        (value, Some(NotifyOn::Some { delay, .. }))
                            if value.eq_ignore_ascii_case(DELAY) =>
                        {
                            *delay = true;
                        }
                        _ => return Err(ParseArgsError::InvalidArgs),
                    }

                    begin = pos;
                }

                Ok(())
            }
            _ => Err(ParseArgsError::InvalidArgs),
        }
    }
}

impl TryFrom<UnparsedArgs> for RcptToArgs {
    type Error = ParseArgsError;

    #[inline]
    fn try_from(value: UnparsedArgs) -> Result<Self, Self::Error> {
        let value = strip_suffix_crlf!(value);

        let mut args = value
            .split(u8::is_ascii_whitespace)
            .filter(|s| !s.is_empty());

        let mailbox = strip_quote(args.next().ok_or(ParseArgsError::InvalidArgs)?)?;
        let mailbox = if mailbox.is_empty() {
            return Err(ParseArgsError::InvalidArgs);
        } else {
            String::from_utf8(mailbox.to_vec())?
        };

        let mut result = Self {
            forward_path: <Address as std::str::FromStr>::from_str(&mailbox)
                .map_err(|_error| ParseArgsError::InvalidMailAddress { mail: mailbox })?,
            original_forward_path: None,
            notify_on: NotifyOn::Some {
                success: false,
                failure: true,
                delay: false,
            },
        };

        for arg in args {
            if arg.contains(&b'=') {
                result.parse_arguments(arg)?;
            } else {
                return Err(ParseArgsError::InvalidArgs);
            }
        }

        Ok(result)
    }
}

/// SMTP Command.
#[derive(
    Debug, Copy, Clone, PartialEq, Eq, strum::AsRefStr, strum::EnumString, strum::EnumVariantNames,
)]
#[non_exhaustive]
pub enum Verb {
    /// Used to identify the SMTP client to the SMTP server. (historical)
    #[strum(serialize = "HELO ")]
    Helo,
    /// Used to identify the SMTP client to the SMTP server and request smtp extensions.
    #[strum(serialize = "EHLO ")]
    Ehlo,
    /// This command is used to initiate a mail transaction in which the mail
    /// data is delivered to an SMTP server that may, in turn, deliver it to
    /// one or more mailboxes or pass it on to another system (possibly using
    /// SMTP).
    #[strum(serialize = "MAIL FROM:")]
    MailFrom,
    /// This command is used to identify an individual recipient of the mail
    /// data; multiple recipients are specified by multiple uses of this
    /// command.
    #[strum(serialize = "RCPT TO:")]
    RcptTo,
    #[strum(serialize = "DATA\r\n")]
    /// This command causes the mail data to be appended to the mail data
    /// buffer.
    Data,
    /// This command specifies that the receiver MUST send a "221 OK" reply,
    /// and then close the transmission channel.
    #[strum(serialize = "QUIT\r\n")]
    Quit,
    /// This command specifies that the current mail transaction will be
    /// aborted. Any stored sender, recipients, and mail data MUST be
    /// discarded, and all buffers and state tables cleared.
    #[strum(serialize = "RSET\r\n")]
    Rset,
    /// This command causes the server to send helpful information to the
    /// client. The command MAY take an argument (e.g., any command name)
    /// and return more specific information as a response.
    #[strum(serialize = "HELP")]
    Help,
    /// This command does not affect any parameters or previously entered
    /// commands.
    #[strum(serialize = "NOOP\r\n")]
    Noop,
    /// See "Transport Layer Security"
    /// <https://datatracker.ietf.org/doc/html/rfc3207>
    #[strum(serialize = "STARTTLS\r\n")]
    StartTls,
    /// Authentication with SASL protocol
    /// <https://datatracker.ietf.org/doc/html/rfc4954>
    #[strum(serialize = "AUTH ")]
    Auth,
    /// Any other buffer received while expecting a command is considered an
    /// unknown.
    Unknown,
}

impl Verb {
    /// check if the answer of the verb is bufferable (cf. pipelining)
    // Note: missing VRFY, EXPN, TURN
    #[inline]
    #[must_use]
    pub const fn is_bufferable(self) -> bool {
        !matches!(self, Self::Ehlo | Self::Data | Self::Quit | Self::Noop)
    }
}

pub type Batch = Vec<Result<Command<Verb, UnparsedArgs>, Error>>;
