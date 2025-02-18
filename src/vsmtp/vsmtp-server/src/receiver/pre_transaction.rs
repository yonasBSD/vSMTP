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

use crate::{scheduler::Emitter, Handler};
use tokio_rustls::rustls;
use vqueue::GenericQueueManager;
use vsmtp_common::{
    auth::{Credentials, Mechanism},
    status::Status,
    ClientName, Reply,
};
use vsmtp_config::Config;
use vsmtp_mail_parser::MailParser;
use vsmtp_protocol::{
    AcceptArgs, AuthArgs, AuthError, CallbackWrap, ConnectionKind, EhloArgs, HeloArgs,
    ReceiverContext,
};
use vsmtp_rule_engine::{ExecutionStage, RuleEngine, RuleState};

fn build_ehlo_reply(config: &vsmtp_config::Config, is_transaction_secured: bool) -> Reply {
    let auth_mechanism_list: Option<(Vec<Mechanism>, Vec<Mechanism>)> = config
        .server
        .esmtp
        .auth
        .as_ref()
        .map(|auth| auth.mechanisms.iter().partition(|m| m.must_be_under_tls()));

    let esmtp = &config.server.esmtp;

    let auth = if is_transaction_secured {
        // All "unsafe" mechanisms are available under tls.
        auth_mechanism_list.as_ref().map(|(must_be_secured, _)| {
            (
                "250",
                format!(
                    "AUTH {}\r\n",
                    must_be_secured
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(" ")
                ),
            )
        })
    } else {
        auth_mechanism_list.as_ref().map(|(plain, secured)| {
            if config
                .server
                .esmtp
                .auth
                .as_ref()
                .map_or(false, |auth| auth.enable_dangerous_mechanism_in_clair)
            {
                // The user as decided to use unsafe mechanisms, even while not using tls.
                (
                    "250",
                    format!(
                        "AUTH {}\r\n",
                        &[secured.clone(), plain.clone()]
                            .concat()
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(" ")
                    ),
                )
            } else {
                (
                    "250",
                    format!(
                        "AUTH {}\r\n",
                        secured
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(" ")
                    ),
                )
            }
        })
    };

    // FIXME: The following code to create a reply could be cached.
    //       (Expect for the auth and starttls extensions because
    //       they need the transaction context)
    let mut reply = String::default();
    let mut extensions = [
        Some(("250", config.server.name.to_string())),
        auth,
        esmtp
            .eightbitmime
            .then_some(("250", "8BITMIME".to_string())),
        (esmtp.eightbitmime && esmtp.smtputf8).then_some(("250", "SMTPUTF8".to_string())),
        (!is_transaction_secured).then_some(("250", "STARTTLS".to_string())),
        esmtp
            .pipelining
            .then_some(("250", "PIPELINING".to_string())),
        esmtp.chunking.then_some(("250", "CHUNKING".to_string())),
        Some(("250", "DSN".to_owned())),
        Some(("250", format!("SIZE {}", esmtp.size))),
    ]
    .into_iter()
    .flatten()
    .peekable();

    // The hyphen (-), when present as the fourth character of a response,
    // indicates the response is continued on the next line.
    // https://datatracker.ietf.org/doc/html/rfc5321#section-4.1.1.1
    while let Some((code, extension)) = extensions.next() {
        // Last extension, we do not include the hyphen.
        if extensions.peek().is_none() {
            reply.push_str(&format!("{code} {extension}\r\n"));
        } else {
            reply.push_str(&format!("{code}-{extension}\r\n"));
        }
    }

    reply.parse::<Reply>().expect("valid reply")
}

impl<Parser, ParserFactory> Handler<Parser, ParserFactory>
where
    Parser: MailParser + Send + Sync,
    ParserFactory: Fn() -> Parser + Send + Sync,
{
    /// Callback to provided to [`vsmtp_protocol::Receiver`] to handle the connection
    pub fn on_accept(
        AcceptArgs {
            client_addr,
            server_addr,
            timestamp,
            uuid,
            kind,
            ..
        }: AcceptArgs,
        rule_engine: std::sync::Arc<RuleEngine>,
        config: std::sync::Arc<Config>,
        rustls_config: Option<std::sync::Arc<rustls::ServerConfig>>,
        queue_manager: std::sync::Arc<dyn GenericQueueManager>,
        emitter: std::sync::Arc<Emitter>,
        message_parser_factory: ParserFactory,
    ) -> (Self, ReceiverContext, Option<Reply>) {
        let mut ctx = ReceiverContext::default();
        let mut skipped = None;
        let state = rule_engine.spawn_at_connect(
            client_addr,
            server_addr,
            config.server.name.clone(),
            timestamp,
            uuid,
        );

        if rule_engine
            .get_delegation_directive_bound_to_address(server_addr)
            .is_some()
        {
            state
                .context()
                .write()
                .expect("bad state")
                .set_skipped(Status::DelegationResult);
            skipped = Some(Status::DelegationResult);
        }

        let reply = match rule_engine.run_when(&state, &mut skipped, ExecutionStage::Connect) {
            // FIXME: do we really want to let the end-user override the EHLO/HELO reply?
            Status::Faccept(reply) | Status::Accept(reply) => reply,
            Status::Quarantine(_) | Status::Next | Status::DelegationResult => {
                format!("220 {} Service ready\r\n", config.server.name)
                    .parse::<Reply>()
                    .expect("valid")
            }
            Status::Deny(reply) | Status::Reject(reply) => {
                ctx.deny();
                return (
                    Self {
                        config,
                        rustls_config,
                        rule_engine,
                        queue_manager,
                        message_parser_factory,
                        emitter,
                        state,
                        state_internal: None,
                        skipped,
                    },
                    ctx,
                    Some(reply),
                );
            }
            // FIXME: user ran a delegate method before postq/delivery
            Status::Delegated(_) => unreachable!(),
        };

        // NOTE: in that case, the return value is ignored and
        // we have to manually trigger the TLS handshake,
        if kind == ConnectionKind::Tunneled
            && !state.context().read().expect("state poisoned").is_secured()
        {
            match &rustls_config {
                Some(config) => ctx.upgrade_tls(config.clone(), std::time::Duration::from_secs(2)),
                None => ctx.deny(),
            }
            return (
                Self {
                    config,
                    rustls_config,
                    rule_engine,
                    queue_manager,
                    message_parser_factory,
                    emitter,
                    state,
                    state_internal: None,
                    skipped,
                },
                ctx,
                None,
            );
        }

        (
            Self {
                config,
                rustls_config,
                rule_engine,
                queue_manager,
                message_parser_factory,
                emitter,
                state,
                state_internal: None,
                skipped,
            },
            ctx,
            Some(reply),
        )
    }

    pub(super) fn generate_sasl_callback_inner(&self) -> CallbackWrap {
        CallbackWrap(Box::new(RsaslSessionCallback {
            rule_engine: self.rule_engine.clone(),
            state: self.state.clone(),
        }))
    }

    pub(super) fn on_post_tls_handshake_inner(
        &mut self,
        sni: Option<String>,
        protocol_version: rustls::ProtocolVersion,
        cipher_suite: rustls::CipherSuite,
        peer_certificates: Option<Vec<rustls::Certificate>>,
        alpn_protocol: Option<Vec<u8>>,
    ) -> Reply {
        let server_name = sni.map(|sni| sni.parse().unwrap());

        self.state
            .context()
            .write()
            .expect("state poisoned")
            .to_secured(
                server_name.clone(),
                protocol_version,
                cipher_suite,
                peer_certificates,
                alpn_protocol,
            )
            .expect("bad state");

        format!(
            "220 {} Service ready\r\n",
            server_name.unwrap_or_else(|| self.config.server.name.clone())
        )
        .parse::<Reply>()
        .unwrap()
    }

    pub(super) fn on_starttls_inner(&mut self, ctx: &mut ReceiverContext) -> Reply {
        if self
            .state
            .context()
            .read()
            .expect("state poisoned")
            .is_secured()
        {
            "554 5.5.1 Error: TLS already active\r\n"
                .parse::<Reply>()
                .unwrap()
        } else {
            self.rustls_config.as_ref().map_or(
                "454 TLS not available due to temporary reason\r\n"
                    .parse::<Reply>()
                    .unwrap(),
                |config| {
                    ctx.upgrade_tls(config.clone(), std::time::Duration::from_secs(2));
                    "220 TLS go ahead\r\n".parse::<Reply>().unwrap()
                },
            )
        }
    }

    pub(super) fn on_auth_inner(
        &mut self,
        ctx: &mut ReceiverContext,
        args: AuthArgs,
    ) -> Option<Reply> {
        if let Some(auth) = &self.config.server.esmtp.auth {
            if !self
                .state
                .context()
                .read()
                .expect("state poisoned")
                .is_secured()
                && args.mechanism.must_be_under_tls()
                && !auth.enable_dangerous_mechanism_in_clair
            {
                return Some(
                    "538 5.7.11 Encryption required for requested authentication mechanism\r\n"
                        .parse::<Reply>()
                        .unwrap(),
                );
            }

            ctx.authenticate(args.mechanism, args.initial_response);

            None
        } else {
            Some("502 Command not implemented\r\n".parse::<Reply>().unwrap())
        }
    }

    pub(super) fn on_post_auth_inner(
        &mut self,
        ctx: &mut ReceiverContext,
        result: Result<(), AuthError>,
    ) -> Reply {
        match result {
            Ok(()) => {
                self.state
                    .context()
                    .write()
                    .expect("state poisoned")
                    .auth_mut()
                    .expect("bad state")
                    .authenticated = true;

                "235 2.7.0 Authentication succeeded\r\n"
                    .parse::<Reply>()
                    .unwrap()
            }
            Err(AuthError::ClientMustNotStart) => {
                "501 5.7.0 Client must not start with this mechanism\r\n"
                    .parse::<Reply>()
                    .unwrap()
            }
            Err(AuthError::ValidationError(..)) => {
                ctx.deny();
                "535 5.7.8 Authentication credentials invalid\r\n"
                    .parse::<Reply>()
                    .unwrap()
            }
            Err(AuthError::Canceled) => {
                let state = self.state.context();
                let mut guard = state.write().expect("state poisoned");
                let auth_properties = guard.to_auth().expect("bad state");

                auth_properties.cancel_count += 1;
                let attempt_count_max = self
                    .config
                    .server
                    .esmtp
                    .auth
                    .as_ref()
                    .map_or(-1, |auth| auth.attempt_count_max);

                if attempt_count_max != -1
                    && auth_properties.cancel_count >= attempt_count_max.try_into().unwrap()
                {
                    ctx.deny();
                }

                "501 Authentication canceled by client\r\n"
                    .parse::<Reply>()
                    .unwrap()
            }
            Err(AuthError::Base64 { .. }) => "501 5.5.2 Invalid, not base64\r\n"
                .parse::<Reply>()
                .unwrap(),
            Err(AuthError::SessionError(e)) => {
                tracing::warn!(%e, "auth error");
                ctx.deny();
                "454 4.7.0 Temporary authentication failure\r\n"
                    .parse::<Reply>()
                    .unwrap()
            }
            Err(AuthError::IO(e)) => todo!("{e}"),
            Err(AuthError::ConfigError(rsasl::prelude::SASLError::NoSharedMechanism)) => {
                ctx.deny();
                "504 5.5.4 Mechanism is not supported\r\n"
                    .parse::<Reply>()
                    .unwrap()
            }
            Err(AuthError::ConfigError(e)) => todo!("handle non_exhaustive pattern: {e}"),
        }
    }

    pub(super) fn on_helo_inner(&mut self, ctx: &mut ReceiverContext, args: HeloArgs) -> Reply {
        self.state
            .context()
            .write()
            .expect("state poisoned")
            .to_helo(ClientName::Domain(args.client_name), true)
            .expect("bad state");

        match self
            .rule_engine
            .run_when(&self.state, &mut self.skipped, ExecutionStage::Helo)
        {
            Status::Faccept(reply) | Status::Accept(reply) => reply,
            Status::Quarantine(_) | Status::Next | Status::DelegationResult => {
                "250 Ok\r\n".parse::<Reply>().unwrap()
            }
            Status::Deny(reply) | Status::Reject(reply) => {
                ctx.deny();
                reply
            }
            // FIXME: user ran a delegate method before postq/delivery
            Status::Delegated(_) => unreachable!(),
        }
    }

    /// Create a reply for the EHLO command, taking into account enabled/disabled
    /// extensions from the vsl configuration.

    pub(super) fn on_ehlo_inner(&mut self, ctx: &mut ReceiverContext, args: EhloArgs) -> Reply {
        let vsl_ctx = self.state.context();

        vsl_ctx
            .write()
            .expect("state poisoned")
            .to_helo(args.client_name, false)
            .expect("bad state");

        match self
            .rule_engine
            .run_when(&self.state, &mut self.skipped, ExecutionStage::Helo)
        {
            Status::Faccept(reply) | Status::Accept(reply) => reply,
            Status::Quarantine(_) | Status::Next | Status::DelegationResult => {
                let ctx = vsl_ctx.read().expect("state poisoned");

                build_ehlo_reply(&self.state.server().config, ctx.is_secured())
            }
            Status::Deny(reply) | Status::Reject(reply) => {
                ctx.deny();
                reply
            }
            // FIXME: user ran a delegate method before postq/delivery
            Status::Delegated(_) => unreachable!(),
        }
    }
}

///
pub struct ValidationVSL;

impl rsasl::validate::Validation for ValidationVSL {
    type Value = ();
}

#[derive(Debug, thiserror::Error)]
pub enum ValidationError {
    #[error(
        "the rules at stage '{}' returned non '{}' status",
        ExecutionStage::Authenticate,
        Status::Accept("250 Ok\r\n".parse::<Reply>().unwrap()).as_ref()
    )]
    NonAcceptCode,
}

struct RsaslSessionCallback {
    rule_engine: std::sync::Arc<RuleEngine>,
    state: std::sync::Arc<RuleState>,
}

impl RsaslSessionCallback {
    #[allow(clippy::unnecessary_wraps)]
    fn inner_validate(
        &self,
        credentials: Credentials,
    ) -> Result<<ValidationVSL as rsasl::validate::Validation>::Value, ValidationError> {
        self.state
            .context()
            .write()
            .expect("state poisoned")
            .with_credentials(credentials)
            .expect("bad state");

        let mut skipped = None;
        let result =
            self.rule_engine
                .run_when(&self.state, &mut skipped, ExecutionStage::Authenticate);

        if !matches!(result, Status::Accept(..)) {
            return Err(ValidationError::NonAcceptCode);
        }

        Ok(())
    }
}

impl rsasl::callback::SessionCallback for RsaslSessionCallback {
    fn callback(
        &self,
        _session_data: &rsasl::callback::SessionData,
        _context: &rsasl::callback::Context<'_>,
        _request: &mut rsasl::callback::Request<'_>,
    ) -> Result<(), rsasl::prelude::SessionError> {
        Ok(())
    }

    fn validate(
        &self,
        session_data: &rsasl::callback::SessionData,
        context: &rsasl::callback::Context<'_>,
        validate: &mut rsasl::validate::Validate<'_>,
    ) -> Result<(), rsasl::validate::ValidationError> {
        let credentials = Credentials::try_from((session_data, context)).map_err(|e| match e {
            vsmtp_common::auth::Error::MissingField => {
                rsasl::validate::ValidationError::MissingRequiredProperty
            }
            otherwise => rsasl::validate::ValidationError::Boxed(Box::new(otherwise)),
        })?;

        validate.with::<ValidationVSL, _>(|| {
            self.inner_validate(credentials)
                .map_err(|e| rsasl::validate::ValidationError::Boxed(Box::new(e)))
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use vsmtp_config::field::FieldServerESMTP;

    use super::*;

    #[test]
    fn build_full_ehlo() {
        let config = vsmtp_config::Config::builder()
            .with_version_str("<1.0.0")
            .unwrap()
            .without_path()
            .with_server_name("testserver.com".parse::<vsmtp_common::Domain>().unwrap())
            .with_user_group_and_default_system("root", "root")
            .unwrap()
            .with_ipv4_localhost()
            .with_default_logs_settings()
            .with_spool_dir_and_default_queues("./tmp/spool")
            .without_tls_support()
            .with_default_smtp_options()
            .with_default_smtp_error_handler()
            .with_default_extensions()
            .with_app_at_location("./tmp/app")
            .with_vsl(format!(
                "{}/src/template/ignore_vsl/domain-enabled",
                env!("CARGO_MANIFEST_DIR")
            ))
            .with_default_app_logs()
            .with_system_dns()
            .without_virtual_entries()
            .validate();
        let reply = build_ehlo_reply(&config, true);
        assert_eq!(reply.code().value(), 250);
        assert_eq!(
            reply.to_string(),
            [
                "250-testserver.com",
                "250-8BITMIME",
                "250-SMTPUTF8",
                "250-PIPELINING",
                "250-DSN",
                "250 SIZE 20000000\r\n",
            ]
            .join("\r\n")
        );
        // build_ehlo_reply(config: &vsmtp_config::Config, is_transaction_secured: bool)
    }

    #[test]
    fn build_ehlo_without_8bit() {
        let extensions = FieldServerESMTP {
            auth: None,
            eightbitmime: false,
            smtputf8: true,
            pipelining: true,
            chunking: false,
            size: 10,
        };
        let config = vsmtp_config::Config::builder()
            .with_version_str("<1.0.0")
            .unwrap()
            .without_path()
            .with_server_name("testserver.com".parse::<vsmtp_common::Domain>().unwrap())
            .with_user_group_and_default_system("root", "root")
            .unwrap()
            .with_ipv4_localhost()
            .with_default_logs_settings()
            .with_spool_dir_and_default_queues("./tmp/spool")
            .without_tls_support()
            .with_default_smtp_options()
            .with_default_smtp_error_handler()
            .with_extensions(extensions)
            .with_app_at_location("./tmp/app")
            .with_vsl(format!(
                "{}/src/template/ignore_vsl/domain-enabled",
                env!("CARGO_MANIFEST_DIR")
            ))
            .with_default_app_logs()
            .with_system_dns()
            .without_virtual_entries()
            .validate();
        let reply = build_ehlo_reply(&config, true);
        assert_eq!(reply.code().value(), 250);
        assert_eq!(
            reply.to_string(),
            [
                "250-testserver.com",
                "250-PIPELINING",
                "250-DSN",
                "250 SIZE 10\r\n",
            ]
            .join("\r\n")
        );
        // build_ehlo_reply(config: &vsmtp_config::Config, is_transaction_secured: bool)
    }
}
