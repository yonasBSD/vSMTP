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

pub mod api;

/// Export the vsmtp_plugin_redis module.
#[allow(improper_ctypes_definitions)]
#[no_mangle]
pub extern "C" fn module_entrypoint() -> rhai::Shared<rhai::Module> {
    rhai::config::hashing::set_ahash_seed(Some([1, 2, 3, 4])).unwrap();

    #[cfg(debug_assertions)]
    {
        dbg!(std::any::TypeId::of::<rhai::Map>());
    }

    rhai::exported_module!(api::vsmtp_plugin_redis).into()
}
