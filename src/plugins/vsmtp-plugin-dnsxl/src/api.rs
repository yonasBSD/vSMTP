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

use std::str::FromStr;

use rhai::plugin::{
    mem, Dynamic, FnAccess, FnNamespace, Module, NativeCallContext, PluginFunction, RhaiResult,
    TypeId,
};
use strum_macros::EnumString;
use trust_dns_resolver::config::{ResolverConfig, ResolverOpts};
use trust_dns_resolver::Resolver;

#[derive(Debug, EnumString)]
enum BlockListKind {
    #[strum(ascii_case_insensitive)]
    Spamhaus,
    #[strum(ascii_case_insensitive)]
    Spamrats,
    #[strum(ascii_case_insensitive)]
    Spamcops,
    #[strum(ascii_case_insensitive)]
    Lashback,
    #[strum(ascii_case_insensitive)]
    S5h,
    #[strum(ascii_case_insensitive)]
    Sorbs,
    #[strum(ascii_case_insensitive)]
    BackScatterer,
    #[strum(ascii_case_insensitive)]
    Singular,
}

impl BlockListKind {
    fn to_url(&self) -> String {
        match self {
            Self::Spamhaus => String::from("zen.spamhaus.org"),
            Self::Spamrats => String::from("all.spamrats.com"),
            Self::Spamcops => String::from("bl.spamcop.net"),
            Self::Lashback => String::from("ubl.unsubscore.com"),
            Self::S5h => String::from("all.s5h.net"),
            Self::Sorbs => String::from("dnsxl.sorbs.net"),
            Self::BackScatterer => String::from("ips.backscatterer.org"),
            Self::Singular => String::from("singular.ttk.pte.hu"),
        }
    }
}

#[derive(Debug, serde::Deserialize)]
struct DnsxlParameters {
    #[serde(default)]
    pub bl: Vec<String>,
    #[serde(default)]
    pub wl: Vec<String>,
}

pub struct Dnsbl {
    pub bl: Vec<String>,
    resolver: Resolver,
}

pub struct Dnswl {
    pub wl: Vec<String>,
    resolver: Resolver,
}

impl Dnsbl {
    pub fn contains(&self, domain: &str, map: &mut rhai::Map) -> bool {
        let mut result = false;
        for element in &self.bl {
            let bl_kind = BlockListKind::from_str(element);
            if let Ok(kind) = bl_kind {
                let response = self
                    .resolver
                    .lookup_ip(domain.to_owned() + "." + kind.to_url().as_str());
                if let Ok(ips) = response {
                    map.insert(
                        element.into(),
                        ips.iter()
                            .map(|x| x.to_string())
                            .collect::<Vec<String>>()
                            .into(),
                    );
                    result = true;
                }
            } else {
                let response = self
                    .resolver
                    .lookup_ip(domain.to_owned() + "." + element.as_str());
                if let Ok(ips) = response {
                    map.insert(
                        element.into(),
                        ips.iter()
                            .map(|x| x.to_string())
                            .collect::<Vec<String>>()
                            .into(),
                    );
                    result = true;
                }
            }
        }
        result
    }
}

impl Dnswl {
    pub fn contains(&self, domain: &str, map: &mut rhai::Map) -> bool {
        let mut result = false;
        for element in &self.wl {
            let response = self
                .resolver
                .lookup_ip(domain.to_owned() + "." + element.as_str());
            if let Ok(ips) = response {
                map.insert(
                    element.into(),
                    ips.iter()
                        .map(|x| x.to_string())
                        .collect::<Vec<String>>()
                        .into(),
                );
                result = true;
            }
        }
        result
    }
}

#[rhai::plugin::export_module]
pub mod vsmtp_plugin_dnsxl {
    pub type Bl = rhai::Shared<Dnsbl>;
    pub type Wl = rhai::Shared<Dnswl>;

    /// Builds a blacklist dns checker.
    ///
    /// # Args
    ///
    /// * `parameters` - a map of the following parameters:
    ///     * `bl` - an array of string corresponding to the different blocklists to use. You can put either keywords or urls.
    ///     The following keywords are available: `spamhaus`, `spamrats`, `spamcops`, `lashback`, `s5h`, `sorbs`, `backscatterer`, `singular`.
    ///
    /// # Return
    ///
    /// A service to check if an IP is blacklisted.
    ///
    /// # Error
    ///
    /// * The service failed to build, or to connect to the resolver.
    ///
    /// # Example
    ///
    /// ```text
    /// // Import the plugin stored in the `plugins` directory.
    /// import "plugins/libvsmtp_plugin_dnsxl" as dnsxl;
    ///
    /// export const my_blacklist = dnsxl::build(#{
    ///     bl: ["spamhaus", "all.s5h.net", "spamrats"],
    /// });
    /// ```
    #[rhai_fn(global, return_raw)]
    pub fn blacklist(parameters: rhai::Map) -> Result<Bl, Box<rhai::EvalAltResult>> {
        let parameters = rhai::serde::from_dynamic::<DnsxlParameters>(&parameters.into())?;

        match Resolver::new(ResolverConfig::default(), ResolverOpts::default()) {
            Ok(res) => Ok(rhai::Shared::new(Dnsbl {
                resolver: res,
                bl: parameters.bl,
            })),
            Err(e) => {
                Err(e).map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?
            }
        }
    }

    /// Builds a whitelist dns checker.
    ///
    /// # Args
    ///
    /// * `parameters` - a map of the following parameters:
    ///     * `wl` - an array of string corresponding to the different whitelists to use.
    ///
    /// # Return
    ///
    /// A service to check if an IP is whitelisted.
    ///
    /// # Error
    ///
    /// * The service failed to build, or to connect to the resolver.
    ///
    /// # Example
    ///
    /// ```text
    /// // Import the plugin stored in the `plugins` directory.
    /// import "plugins/libvsmtp_plugin_dnsxl" as dnsxl;
    ///
    /// export const my_whitelist = dnsxl::build(#{
    ///     wl: ["localhost"],
    /// });
    /// ```
    #[rhai_fn(global, return_raw)]
    pub fn whitelist(parameters: rhai::Map) -> Result<Wl, Box<rhai::EvalAltResult>> {
        let parameters = rhai::serde::from_dynamic::<DnsxlParameters>(&parameters.into())?;

        match Resolver::new(ResolverConfig::default(), ResolverOpts::default()) {
            Ok(res) => Ok(rhai::Shared::new(Dnswl {
                resolver: res,
                wl: parameters.wl,
            })),
            Err(e) => {
                Err(e).map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?
            }
        }
    }

    /// Successively checks through the blacklists provided if the IP is inside.
    ///
    /// # Args
    ///
    /// * `domain` - The IP you want to check.
    ///
    /// # Return
    ///
    /// A map containing the name of the blacklists or the sites where the IP was found, with their own return code.
    /// If nothing is found, a rhai UNIT is returned.
    ///
    /// # Example
    ///
    /// Build a service in `services/dnsxl.vsl`;
    ///
    /// ```text
    /// // Import the plugin stored in the `plugins` directory.
    /// import "plugins/libvsmtp_plugin_dnsxl" as dnsxl;
    ///
    /// export const my_blacklist = dnsxl::blacklist(#{
    ///     bl: ["spamhaus", "all.s5h.net"],
    /// });
    /// ```
    ///
    /// Check if the value is contained in the list provided.
    ///
    /// ```text
    /// import "services/dnsxl" as srv;
    ///
    /// #{
    ///     connect: [
    ///         action "checking if my ip is blacklisted" || {
    ///             let res = srv::my_blacklist.contains("2.0.0.127");
    ///             // Checking if a map is returned to see if the IP was found through the blacklists provided.
    ///             if (res != ()) {
    ///                 log("info", "2.0.0.127 is blacklisted");
    ///                 // Having a look in the map to see what return codes I have.
    ///                 for record in res["all.s5h.net"] {
    ///                     log("info", `code -> ${record}`);
    ///                 }
    ///             }
    ///         }
    ///     ],
    /// }
    /// ```
    #[rhai_fn(global, name = "contains", pure)]
    pub fn contains_bl(con: &mut Bl, domain: rhai::Dynamic) -> rhai::Dynamic {
        let mut map = rhai::Map::new();
        if con.contains(domain.to_string().as_str(), &mut map) {
            rhai::Dynamic::from_map(map)
        } else {
            rhai::Dynamic::UNIT
        }
    }

    /// Successively checks through the whitelists provided if the IP is inside.
    ///
    /// # Args
    ///
    /// * `domain` - The IP you want to check.
    ///
    /// # Return
    ///
    /// A map containing the name of the whitelists where the IP was found, with their own return code.
    /// If nothing is found, a rhai UNIT is returned.
    ///
    /// # Example
    ///
    /// Build a service in `services/dnsxl.vsl`;
    ///
    /// ```text
    /// // Import the plugin stored in the `plugins` directory.
    /// import "plugins/libvsmtp_plugin_dnsxl" as dnsxl;
    ///
    /// export const my_blacklist = dnsxl::blacklist(#{
    ///     wl: ["localhost"],
    /// });
    /// ```
    ///
    /// Check if the value is contained in the list provided.
    ///
    /// ```text
    /// import "services/dnsxl" as srv;
    ///
    /// #{
    ///     connect: [
    ///         action "checking if my ip is whitelisted" || {
    ///             let res = srv::my_whitelist.contains("2.0.0.127");
    ///             // Checking if a map is returned to see if the IP was found through the whitelists provided.
    ///             if (res != ()) {
    ///                 log("info", "2.0.0.127 is whitelisted");
    ///                 // Having a look in the map to see what return codes I have.
    ///                 for record in res["localhost"] {
    ///                     log("info", `code -> ${record}`);
    ///                 }
    ///             }
    ///         }
    ///     ],
    /// }
    /// ```
    #[rhai_fn(global, name = "contains", pure)]
    pub fn contains_wl(con: &mut Wl, domain: rhai::Dynamic) -> rhai::Dynamic {
        let mut map = rhai::Map::new();
        if con.contains(domain.to_string().as_str(), &mut map) {
            rhai::Dynamic::from_map(map)
        } else {
            rhai::Dynamic::UNIT
        }
    }
}
