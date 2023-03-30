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

use crate::api::{EngineResult, SharedObject};
use rhai::plugin::{
    mem, Dynamic, EvalAltResult, FnAccess, FnNamespace, ImmutableString, Module, NativeCallContext,
    PluginFunction, RhaiResult, TypeId,
};
use vsmtp_common::{status::Status, Reply};
use vsmtp_plugin_vsl::objects::Object;

fn reply_or_code_id_from_object(code: &SharedObject) -> EngineResult<Reply> {
    match &**code {
        Object::Code(reply) => Ok(reply.clone()),
        object => Err(format!("parameter must be a code, not {}", object.as_ref()).into()),
    }
}

fn reply_or_code_id_from_string(code: &str) -> EngineResult<Reply> {
    <Reply as std::str::FromStr>::from_str(code).map_err::<Box<EvalAltResult>, _>(|_| {
        format!("parameter must be a code, not {code:?}").into()
    })
}

pub use state::*;

/// Functions used to interact with the rule engine.
/// Use `states` in `rules` to deny, accept, or quarantine emails.
#[rhai::plugin::export_module]
mod state {
    /// Tell the rule engine to force accept the incoming transaction.
    /// This means that all rules following the one `faccept` is called
    /// will be ignored.
    ///
    /// Use this return status when you are sure that
    /// the incoming client can be trusted.
    ///
    /// # Args
    ///
    /// * code - A customized code as a string or code object. (default: "250 Ok")
    ///
    /// # Errors
    ///
    /// * The object passed as parameter was not a code object.
    /// * The string passed as parameter failed to be parsed into a valid code.
    ///
    /// # Effective smtp stage
    ///
    /// all of them.
    ///
    /// # Example
    ///
    /// ```ignore
    /// #{
    ///     connect: [
    ///         // Here we imagine that "192.168.1.10" is a trusted source, so we can force accept
    ///         // any other rules that don't need to be run.
    ///         rule "check for trusted source" || if ctx::client_ip() == "192.168.1.10" { faccept() } else { state::next() },
    ///     ],
    ///
    ///     // The following rules will not be evaluated if `ctx::client_ip() == "192.168.1.10"` is true.
    ///     mail: [
    ///         rule "another rule" || {
    ///             // ... doing stuff
    ///         }
    ///     ],
    /// }
    ///
    /// #{
    ///     mail: [
    ///         rule "send a custom code with a code object" || {
    ///             faccept(code(220, "Ok"))
    ///         }
    ///     ],
    /// }
    ///
    /// #{
    ///     mail: [
    ///         rule "send a custom code with a string" || {
    ///             faccept("220 Ok")
    ///         }
    ///     ],
    /// }
    /// ```
    ///
    /// # rhai-autodocs:index:1
    #[must_use]
    pub fn faccept() -> Status {
        Status::Faccept("250 Ok\r\n".parse::<Reply>().unwrap())
    }

    #[doc(hidden)]
    #[rhai_fn(name = "faccept", return_raw, pure)]
    pub fn faccept_with_code(code: &mut SharedObject) -> EngineResult<Status> {
        reply_or_code_id_from_object(code).map(Status::Faccept)
    }

    #[doc(hidden)]
    #[rhai_fn(name = "faccept", return_raw)]
    pub fn faccept_with_string(code: &str) -> EngineResult<Status> {
        reply_or_code_id_from_string(code).map(Status::Faccept)
    }

    /// Tell the rule engine to accept the incoming transaction for the current stage.
    /// This means that all rules following the one `accept` is called in the current stage
    /// will be ignored.
    ///
    /// # Args
    ///
    /// * code - A customized code as a string or code object. (default: "250 Ok")
    ///
    /// # Errors
    ///
    /// * The object passed as parameter was not a code object.
    /// * The string passed as parameter failed to be parsed into a valid code.
    ///
    /// # Effective smtp stage
    ///
    /// all of them.
    ///
    /// # Example
    ///
    /// ```ignore
    /// #{
    ///     connect: [
    ///         // "ignored checks" will be ignored because the previous rule returned accept.
    ///         rule "accept" || state::accept(),
    ///         action "ignore checks" || print("this will be ignored because the previous rule used state::accept()."),
    ///     ],
    ///
    ///     mail: [
    ///         // rule evaluation is resumed in the next stage.
    ///         rule "resume rules" || print("evaluation resumed!");
    ///     ]
    /// }
    ///
    /// #{
    ///     mail: [
    ///         rule "send a custom code with a code object" || {
    ///             accept(code(220, "Ok"))
    ///         }
    ///     ],
    /// }
    ///
    /// #{
    ///     mail: [
    ///         rule "send a custom code with a string" || {
    ///             accept("220 Ok")
    ///         }
    ///     ],
    /// }
    /// ```
    ///
    /// # rhai-autodocs:index:2
    #[must_use]
    pub fn accept() -> Status {
        Status::Accept("250 Ok\r\n".parse::<Reply>().unwrap())
    }

    #[doc(hidden)]
    #[rhai_fn(name = "accept", return_raw, pure)]
    pub fn accept_with_code(code: &mut SharedObject) -> EngineResult<Status> {
        reply_or_code_id_from_object(code).map(Status::Accept)
    }

    #[doc(hidden)]
    #[rhai_fn(name = "accept", return_raw)]
    pub fn accept_with_string(code: &str) -> EngineResult<Status> {
        reply_or_code_id_from_string(code).map(Status::Accept)
    }

    /// Tell the rule engine that a rule succeeded. Following rules
    /// in the current stage will be executed.
    ///
    /// # Effective smtp stage
    ///
    /// all of them.
    ///
    /// # Example
    ///
    /// ```ignore
    /// #{
    ///     connect: [
    ///         // once "go to the next rule" is evaluated, the rule engine execute "another rule".
    ///         rule "go to the next rule" || state::next(),
    ///         action "another rule" || print("checking stuff ..."),
    ///     ],
    /// }
    /// ```
    ///
    /// # rhai-autodocs:index:3
    #[must_use]
    pub const fn next() -> Status {
        Status::Next
    }

    /// Stop rules evaluation and send an error code to the client.
    ///
    /// # Args
    ///
    /// * code - A customized code as a string or code object. (default: "554 permanent problems with the remote server")
    ///
    /// # Errors
    ///
    /// * The object passed as parameter was not a code object.
    /// * The string passed as parameter failed to be parsed into a valid code.
    ///
    /// # Effective smtp stage
    ///
    /// all of them.
    ///
    /// # Example
    ///
    /// ```ignore
    /// #{
    ///     rcpt: [
    ///         rule "check for satan" || {
    ///            // The client is denied if a recipient's domain matches satan.org,
    ///            // this is a blacklist, sort-of.
    ///            if ctx::rcpt().domain == "satan.org" {
    ///                state::deny()
    ///            } else {
    ///                state::next()
    ///            }
    ///        },
    ///     ],
    /// }
    ///
    /// #{
    ///     mail: [
    ///         rule "send a custom code with a code object" || {
    ///             deny(code(421, "Service not available"))
    ///         }
    ///     ],
    /// }
    ///
    /// #{
    ///     mail: [
    ///         rule "send a custom code with a string" || {
    ///             deny("450 mailbox unavailable")
    ///         }
    ///     ],
    /// }
    /// ```
    ///
    /// # rhai-autodocs:index:4
    #[must_use]
    #[rhai_fn(global)]
    pub fn deny() -> Status {
        Status::Deny(
            "554 permanent problems with the remote server\r\n"
                .parse::<Reply>()
                .unwrap(),
        )
    }

    #[doc(hidden)]
    #[rhai_fn(name = "deny", return_raw, pure)]
    pub fn deny_with_code(code: &mut SharedObject) -> EngineResult<Status> {
        reply_or_code_id_from_object(code).map(Status::Deny)
    }

    #[doc(hidden)]
    #[rhai_fn(name = "deny", return_raw)]
    pub fn deny_with_string(code: &str) -> EngineResult<Status> {
        reply_or_code_id_from_string(code).map(Status::Deny)
    }

    /// Skip all rules until the email is received and place the email in a
    /// quarantine queue. The email will never be sent to the recipients and
    /// will stop being processed after the `PreQ` stage.
    ///
    /// # Args
    ///
    /// * `queue` - the relative path to the queue where the email will be quarantined as a string.
    ///             This path will be concatenated to the `config.app.dirpath` field in
    ///             your root configuration.
    ///
    /// # Effective smtp stage
    ///
    /// all of them.
    ///
    /// # Example
    ///
    /// ```ignore
    /// import "services" as svc;
    ///
    /// #{
    ///     postq: [
    ///           delegate svc::clamsmtpd "check email for virus" || {
    ///               // the email is placed in quarantined if a virus is detected by
    ///               // a service.
    ///               if has_header("X-Virus-Infected") {
    ///                 state::quarantine("virus_queue")
    ///               } else {
    ///                 state::next()
    ///               }
    ///           }
    ///     ],
    /// }
    /// ```
    ///
    /// # rhai-autodocs:index:5
    #[must_use]
    #[rhai_fn(name = "quarantine")]
    pub fn quarantine_str(queue: &str) -> Status {
        Status::Quarantine(queue.to_string())
    }

    /// Check if two statuses are equal.
    ///
    /// # Effective smtp stage
    ///
    /// all of them.
    ///
    /// # Example
    ///
    /// ```ignore
    /// #{
    ///     connect: [
    ///         action "check status equality" || {
    ///             deny() == deny(); // returns true.
    ///             faccept() == next(); // returns false.
    ///         }
    ///     ],
    /// }
    /// ```
    ///
    /// # rhai-autodocs:index:6
    #[rhai_fn(global, name = "==", pure)]
    pub fn eq_status_operator(status_1: &mut Status, status_2: Status) -> bool {
        *status_1 == status_2
    }

    /// Check if two statuses are not equal.
    ///
    /// # Effective smtp stage
    ///
    /// all of them.
    ///
    /// # Example
    ///
    /// ```ignore
    /// #{
    ///     connect: [
    ///         action "check status not equal" || {
    ///             deny() != deny(); // returns false.
    ///             faccept() != next(); // returns true.
    ///         }
    ///     ],
    /// }
    /// ```
    ///
    /// # rhai-autodocs:index:7
    #[rhai_fn(global, name = "!=", pure)]
    pub fn neq_status_operator(status_1: &mut Status, status_2: Status) -> bool {
        !(*status_1 == status_2)
    }

    /// Convert a status to a string.
    /// Enables string interpolation.
    ///
    /// # Effective smtp stage
    ///
    /// all of them.
    ///
    /// # Example
    ///
    /// ```text,ignore
    /// #{
    ///     connect: [
    ///         rule "status to string" || {
    ///             let status = next();
    ///             // `.to_string` is called automatically here.
    ///             log("info", `converting my status to a string: ${status}`);
    ///             status
    ///         }
    ///     ],
    /// }
    /// ```
    ///
    /// # rhai-autodocs:index:8
    #[rhai_fn(global, pure)]
    pub fn to_string(status: &mut Status) -> String {
        status.as_ref().to_string()
    }

    /// Convert a status to a debug string
    /// Enables string interpolation.
    ///
    /// # Effective smtp stage
    ///
    /// all of them.
    ///
    /// # Example
    ///
    /// ```text,ignore
    /// #{
    ///     connect: [
    ///         rule "status to string" || {
    ///             let status = next();
    ///             log("info", `converting my status to a string: ${status.to_debug()}`);
    ///             status
    ///         }
    ///     ],
    /// }
    /// ```
    ///
    /// # rhai-autodocs:index:9
    #[rhai_fn(global, pure)]
    pub fn to_debug(status: &mut Status) -> String {
        status.as_ref().to_string()
    }
}
