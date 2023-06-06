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

//! # vSMTP dnsxl plugin

#![doc(html_no_source)]
#![deny(missing_docs)]
#![deny(unsafe_code)]
//
#![warn(rust_2018_idioms)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]
#![warn(clippy::nursery)]
#![warn(clippy::cargo)]
// #![warn(clippy::restriction)]
// restriction we ignore
#![allow(
    clippy::blanket_clippy_restriction_lints,
    clippy::implicit_return,
    clippy::missing_docs_in_private_items,
    clippy::shadow_reuse
)]

mod api;
#[cfg(test)]
mod tests;

/// Entry point of the `dnsxl` plugin
///
/// # Panics
///
/// * the `rhai` hashing seed cannot be set.
#[allow(unsafe_code)]
#[allow(improper_ctypes_definitions)]
#[no_mangle]
pub extern "C" fn module_entrypoint() -> rhai::Shared<rhai::Module> {
    rhai::config::hashing::set_ahash_seed(Some([1, 2, 3, 4])).unwrap();

    #[cfg(debug_assertions)]
    {
        dbg!(std::any::TypeId::of::<rhai::Map>());
    }

    rhai::exported_module!(api::vsmtp_plugin_dnsxl).into()
}
