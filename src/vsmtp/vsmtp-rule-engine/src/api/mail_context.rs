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
    api::{
        EngineResult, {Context, SharedObject},
    },
    get_global,
};
use rhai::plugin::{
    Dynamic, FnAccess, FnNamespace, Module, NativeCallContext, PluginFunction, RhaiResult, TypeId,
};
use vsmtp_plugin_vsl::objects::Object;

pub use mail_context::*;

/// Inspect the transaction context.
#[rhai::plugin::export_module]
mod mail_context {

    /// Produce a serialized JSON representation of the mail context.
    ///
    /// # rhai-autodocs:index:1
    #[rhai_fn(global, pure, return_raw)]
    pub fn to_string(context: &mut Context) -> EngineResult<String> {
        let guard = vsl_guard_ok!(context.read());
        serde_json::to_string_pretty(&*guard)
            .map_err::<Box<rhai::EvalAltResult>, _>(|e| e.to_string().into())
    }

    /// Get the address of the client.
    ///
    /// # Effective smtp stage
    ///
    /// All of them.
    ///
    /// # Return
    ///
    /// * `string` - the client's address with the `ip:port` format.
    ///
    /// # Examples
    ///
    ///```
    /// # vsmtp_test::vsl::run(
    /// # |builder| Ok(builder.add_root_filter_rules(r#"
    /// #{
    ///   connect: [
    ///     action "log client address" || {
    ///       log("info", `new client: ${ctx::client_address()}`);
    ///     },
    ///   ],
    /// }
    /// # "#)?.build()));
    /// ```
    ///
    /// # rhai-autodocs:index:2
    #[rhai_fn(name = "client_address", return_raw)]
    pub fn client_address(ncc: NativeCallContext) -> EngineResult<String> {
        Ok(vsl_guard_ok!(get_global!(ncc, ctx)?.read())
            .client_addr()
            .to_string())
    }

    /// Get the ip address of the client.
    ///
    /// # Effective smtp stage
    ///
    /// All of them.
    ///
    /// # Return
    ///
    /// * `string` - the client's ip address.
    ///
    /// # Example
    ///
    ///```
    /// # vsmtp_test::vsl::run(
    /// # |builder| Ok(builder.add_root_filter_rules(r#"
    /// #{
    ///   connect: [
    ///     action "log client ip" || {
    ///       log("info", `new client: ${ctx::client_ip()}`);
    ///     },
    ///   ],
    /// }
    /// # "#)?.build()));
    /// ```
    ///
    /// # rhai-autodocs:index:3
    #[rhai_fn(name = "client_ip", return_raw)]
    pub fn client_ip(ncc: NativeCallContext) -> EngineResult<String> {
        Ok(vsl_guard_ok!(get_global!(ncc, ctx)?.read())
            .client_addr()
            .ip()
            .to_string())
    }

    /// Get the ip port of the client.
    ///
    /// # Effective smtp stage
    ///
    /// All of them.
    ///
    /// # Return
    ///
    /// * `int` - the client's port.
    ///
    /// # Example
    ///
    ///```
    /// # vsmtp_test::vsl::run(
    /// # |builder| Ok(builder.add_root_filter_rules(r#"
    /// #{
    ///   connect: [
    ///     action "log client address" || {
    ///       log("info", `new client: ${ctx::client_ip()}:${ctx::client_port()}`);
    ///     },
    ///   ],
    /// }
    /// # "#)?.build()));
    /// ```
    ///
    /// # rhai-autodocs:index:4
    #[rhai_fn(name = "client_port", return_raw)]
    pub fn client_port(ncc: NativeCallContext) -> EngineResult<rhai::INT> {
        Ok(rhai::INT::from(
            vsl_guard_ok!(get_global!(ncc, ctx)?.read())
                .client_addr()
                .port(),
        ))
    }

    /// Get the full server address.
    ///
    /// # Effective smtp stage
    ///
    /// All of them.
    ///
    /// # Return
    ///
    /// * `string` - the server's address with the `ip:port` format.
    ///
    /// # Example
    ///
    ///```
    /// # vsmtp_test::vsl::run(
    /// # |builder| Ok(builder.add_root_filter_rules(r#"
    /// #{
    ///   connect: [
    ///     action "log server address" || {
    ///       log("info", `server: ${ctx::server_address()}`);
    ///     },
    ///   ],
    /// }
    /// # "#)?.build()));
    /// ```
    ///
    /// # rhai-autodocs:index:5
    #[rhai_fn(name = "server_address", return_raw)]
    pub fn server_address(ncc: NativeCallContext) -> EngineResult<String> {
        Ok(vsl_guard_ok!(get_global!(ncc, ctx)?.read())
            .server_addr()
            .to_string())
    }

    /// Get the server's ip.
    ///
    /// # Effective smtp stage
    ///
    /// All of them.
    ///
    /// # Return
    ///
    /// * `string` - the server's ip.
    ///
    /// # Example
    ///
    ///```
    /// # vsmtp_test::vsl::run(
    /// # |builder| Ok(builder.add_root_filter_rules(r#"
    /// #{
    ///   connect: [
    ///     action "log server ip" || {
    ///       log("info", `server: ${ctx::server_ip()}`);
    ///     },
    ///   ],
    /// }
    /// # "#)?.build()));
    /// ```
    ///
    /// # rhai-autodocs:index:6
    #[rhai_fn(name = "server_ip", return_raw)]
    pub fn server_ip(ncc: NativeCallContext) -> EngineResult<String> {
        Ok(vsl_guard_ok!(get_global!(ncc, ctx)?.read())
            .server_addr()
            .ip()
            .to_string())
    }

    /// Get the server's port.
    ///
    /// # Effective smtp stage
    ///
    /// All of them.
    ///
    /// # Return
    ///
    /// * `string` - the server's port.
    ///
    /// # Example
    ///
    ///```
    /// # vsmtp_test::vsl::run(
    /// # |builder| Ok(builder.add_root_filter_rules(r#"
    /// #{
    ///   connect: [
    ///     action "log server address" || {
    ///       log("info", `server: ${ctx::server_ip()}:${ctx::server_port()}`);
    ///     },
    ///   ],
    /// }
    /// # "#)?.build()));
    /// ```
    ///
    /// # rhai-autodocs:index:7
    #[rhai_fn(name = "server_port", return_raw)]
    pub fn server_port(ncc: NativeCallContext) -> EngineResult<rhai::INT> {
        Ok(rhai::INT::from(
            vsl_guard_ok!(get_global!(ncc, ctx)?.read())
                .server_addr()
                .port(),
        ))
    }

    /// Get a the timestamp of the client's connection time.
    ///
    /// # Effective smtp stage
    ///
    /// All of them.
    ///
    /// # Return
    ///
    /// * `timestamp` - the connection timestamp of the client.
    ///
    /// # Example
    ///
    ///```
    /// # vsmtp_test::vsl::run(
    /// # |builder| Ok(builder.add_root_filter_rules(r#"
    /// #{
    ///   connect: [
    ///     action "log client" || {
    ///       log("info", `new client connected at ${ctx::connection_timestamp()}`);
    ///     },
    ///   ],
    /// }
    /// # "#)?.build()));
    /// ```
    ///
    /// # rhai-autodocs:index:8
    #[rhai_fn(name = "connection_timestamp", return_raw)]
    pub fn connection_timestamp(ncc: NativeCallContext) -> EngineResult<time::OffsetDateTime> {
        Ok(*vsl_guard_ok!(get_global!(ncc, ctx)?.read()).connection_timestamp())
    }

    /// Get the name of the server.
    ///
    /// # Effective smtp stage
    ///
    /// All of them.
    ///
    /// # Return
    ///
    /// * `string` - the name of the server.
    ///
    /// # Example
    ///
    ///```
    /// # vsmtp_test::vsl::run(
    /// # |builder| Ok(builder.add_root_filter_rules(r#"
    /// #{
    ///   connect: [
    ///     action "log server" || {
    ///       log("info", `server name: ${ctx::server_name()}`);
    ///     },
    ///   ],
    /// }
    /// # "#)?.build()));
    /// ```
    ///
    /// # rhai-autodocs:index:9
    #[rhai_fn(name = "server_name", return_raw)]
    pub fn server_name(ncc: NativeCallContext) -> EngineResult<String> {
        Ok(vsl_guard_ok!(get_global!(ncc, ctx)?.read())
            .server_name()
            .to_string())
    }

    /// Has the connection been secured under the encryption protocol SSL/TLS.
    ///
    /// # Effective smtp stage
    ///
    /// all of them.
    ///
    /// # Return
    ///
    /// * bool - `true` if the connection is secured, `false` otherwise.
    ///
    /// # Example
    ///
    /// ```
    /// # vsmtp_test::vsl::run(
    /// # |builder| Ok(builder.add_root_filter_rules(r#"
    /// #{
    ///   connect: [
    ///     action "log ssl/tls" || {
    ///       log("info", `The client is ${if ctx::is_secured() { "secured" } else { "unsecured!!!" }}`)
    ///     }
    ///   ],
    /// }
    /// # "#)?.build()));
    /// ```
    ///
    /// # rhai-autodocs:index:10
    #[rhai_fn(name = "is_secured", return_raw)]
    pub fn is_secured(ncc: NativeCallContext) -> EngineResult<bool> {
        Ok(vsl_guard_ok!(get_global!(ncc, ctx)?.read()).tls().is_some())
    }

    /// Get the value of the `HELO/EHLO` command sent by the client.
    ///
    /// # Effective smtp stage
    ///
    /// `helo` and onwards.
    ///
    /// # Return
    ///
    /// * `string` - the value of the `HELO/EHLO` command.
    ///
    /// # Examples
    ///
    /// ```
    /// # vsmtp_test::vsl::run(
    /// # |builder| Ok(builder.add_root_filter_rules(r#"
    /// #{
    ///     helo: [
    ///        action "log info" || log("info", `helo/ehlo value: ${ctx::helo()}`),
    ///     ]
    /// }
    /// # "#)?.build()));
    /// ```
    ///
    /// # rhai-autodocs:index:11
    #[rhai_fn(name = "helo", return_raw)]
    pub fn helo(ncc: NativeCallContext) -> EngineResult<String> {
        Ok(vsl_guard_ok!(get_global!(ncc, ctx)?.read())
            .client_name()
            .map(ToString::to_string)
            .map_err(Into::<crate::error::RuntimeError>::into)?)
    }

    /// Get the value of the `MAIL FROM` command sent by the client.
    ///
    /// # Effective smtp stage
    ///
    /// `mail` and onwards.
    /// # Return
    ///
    /// * `address` - the sender address.
    ///
    /// # Examples
    ///
    /// ```
    /// # vsmtp_test::vsl::run(
    /// # |builder| Ok(builder.add_root_filter_rules(r#"
    /// #{
    ///     helo: [
    ///        action "log info" || log("info", `received sender: ${ctx::mail_from()}`),
    ///     ]
    /// }
    /// # "#)?.build()));
    /// ```
    ///
    /// # rhai-autodocs:index:12
    #[rhai_fn(return_raw)]
    pub fn mail_from(ncc: NativeCallContext) -> EngineResult<SharedObject> {
        let reverse_path = vsl_guard_ok!(get_global!(ncc, ctx)?.read())
            .reverse_path()
            .map_err(Into::<crate::error::RuntimeError>::into)?
            .clone();
        Ok(std::sync::Arc::new(reverse_path.map_or_else(
            || Object::Identifier("null".to_string()),
            Object::Address,
        )))
    }

    /// Get the list of recipients received by the client.
    ///
    /// # Effective smtp stage
    ///
    /// `rcpt` and onwards. Note that you will not have all recipients received
    /// all at once in the `rcpt` stage. It is better to use this function
    /// in the later stages.
    ///
    /// # Return
    ///
    /// * `Array of addresses` - the list containing all recipients.
    ///
    /// # Examples
    ///
    /// ```
    /// # vsmtp_test::vsl::run(
    /// # |builder| Ok(builder.add_root_filter_rules(r#"
    /// #{
    ///     preq: [
    ///        action "log recipients" || log("info", `recipients: ${ctx::rcpt_list()}`),
    ///     ]
    /// }
    /// # "#)?.build()));
    /// ```
    ///
    /// # rhai-autodocs:index:13
    #[rhai_fn(name = "rcpt_list", return_raw)]
    pub fn rcpt_list(ncc: NativeCallContext) -> EngineResult<rhai::Array> {
        Ok(vsl_guard_ok!(get_global!(ncc, ctx)?.read())
            .forward_paths()
            .map_err(Into::<crate::error::RuntimeError>::into)?
            .iter()
            .cloned()
            .map(Object::Address)
            .map(std::sync::Arc::new)
            .map(rhai::Dynamic::from)
            .collect())
    }

    /// Get the value of the current `RCPT TO` command sent by the client.
    ///
    /// # Effective smtp stage
    ///
    /// `rcpt` and onwards. Please note that `ctx::rcpt()` will always return
    /// the last recipient received in stages after the `rcpt` stage. Therefore,
    /// this functions is best used in the `rcpt` stage.
    ///
    /// # Return
    ///
    /// * `address` - the address of the received recipient.
    ///
    /// # Examples
    ///
    /// ```
    /// # vsmtp_test::vsl::run(
    /// # |builder| Ok(builder.add_root_filter_rules(r#"
    /// #{
    ///     rcpt: [
    ///        action "log recipients" || log("info", `new recipient: ${ctx::rcpt()}`),
    ///     ]
    /// }
    /// # "#)?.build()));
    /// ```
    ///
    /// # rhai-autodocs:index:14
    #[rhai_fn(name = "rcpt", return_raw)]
    pub fn rcpt(ncc: NativeCallContext) -> EngineResult<SharedObject> {
        let rcpt = vsl_guard_ok!(get_global!(ncc, ctx)?.read())
            .forward_paths()
            .map_err(Into::<crate::error::RuntimeError>::into)?
            .last()
            .ok_or_else(|| crate::error::RuntimeError::Generic {
                message: "recipient are empty".to_string(),
            })?
            .clone();

        Ok(std::sync::Arc::new(Object::Address(rcpt)))
    }

    /// Get the time of reception of the email.
    ///
    /// # Effective smtp stage
    ///
    /// `preq` and onwards.
    ///
    /// # Return
    ///
    /// * `string` - the timestamp.
    ///
    /// # Examples
    ///
    /// ```
    /// # vsmtp_test::vsl::run(
    /// # |builder| Ok(builder.add_root_filter_rules(r#"
    /// #{
    ///     preq: [
    ///        action "receiving the email" || log("info", `time of reception: ${ctx::mail_timestamp()}`),
    ///     ]
    /// }
    /// # "#)?.build()));
    /// ```
    ///
    /// # rhai-autodocs:index:15
    #[rhai_fn(name = "mail_timestamp", return_raw)]
    pub fn mail_timestamp(ncc: NativeCallContext) -> EngineResult<time::OffsetDateTime> {
        Ok(*vsl_guard_ok!(get_global!(ncc, ctx)?.read())
            .mail_timestamp()
            .map_err(Into::<crate::error::RuntimeError>::into)?)
    }

    /// Get the unique id of the received message.
    ///
    /// # Effective smtp stage
    ///
    /// `preq` and onwards.
    ///
    /// # Return
    ///
    /// * `string` - the message id.
    ///
    /// # Examples
    ///
    /// ```
    /// # vsmtp_test::vsl::run(
    /// # |builder| Ok(builder.add_root_filter_rules(r#"
    /// #{
    ///     preq: [
    ///        action "message received" || log("info", `message id: ${ctx::message_id()}`),
    ///     ]
    /// }
    /// # "#)?.build()));
    /// ```
    ///
    /// # rhai-autodocs:index:16
    #[rhai_fn(name = "message_id", return_raw)]
    pub fn message_id(ncc: NativeCallContext) -> EngineResult<String> {
        Ok(vsl_guard_ok!(get_global!(ncc, ctx)?.read())
            .message_uuid()
            .map_err(Into::<crate::error::RuntimeError>::into)?
            .to_string())
    }
}
