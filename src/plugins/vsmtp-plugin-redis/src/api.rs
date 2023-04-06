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

use r2d2_redis::redis::Commands;
use r2d2_redis::RedisConnectionManager;
use rhai::plugin::*;

/// Parameters available for the redis service. Used
/// with serde for easy parsing.
#[derive(Debug, serde::Deserialize)]
struct RedisDatabaseParameters {
    pub url: String,
    #[serde(default = "default_timeout", with = "humantime_serde")]
    pub timeout: std::time::Duration,
    #[serde(default = "default_connections")]
    pub connections: rhai::INT,
}

const fn default_connections() -> rhai::INT {
    4
}

const fn default_timeout() -> std::time::Duration {
    std::time::Duration::from_secs(30)
}

#[derive(Clone)]
/// A redis connector.
pub struct RedisConnector {
    /// The url to the redis server.
    pub url: String,
    /// connection pool for the database.
    pub pool: r2d2_redis::r2d2::Pool<RedisConnectionManager>,
}

struct Wrapper(Dynamic);

impl r2d2_redis::redis::ToRedisArgs for Wrapper {
    fn write_redis_args<W>(&self, out: &mut W)
    where
        W: ?Sized + r2d2_redis::redis::RedisWrite,
    {
        out.write_arg(self.0.to_string().as_bytes());
    }
}

impl r2d2_redis::redis::FromRedisValue for Wrapper {
    fn from_redis_value(v: &r2d2_redis::redis::Value) -> r2d2_redis::redis::RedisResult<Self> {
        match v {
            r2d2_redis::redis::Value::Nil => Ok(Wrapper(rhai::Dynamic::UNIT)),
            r2d2_redis::redis::Value::Int(v) => Ok(Wrapper(rhai::Dynamic::from_int(*v))),
            r2d2_redis::redis::Value::Data(v) => Ok(Wrapper(rhai::Dynamic::from(
                String::from_utf8(v.to_vec()).map_err(|_| {
                    r2d2_redis::redis::RedisError::from((
                        r2d2_redis::redis::ErrorKind::TypeError,
                        "Could not convert data to string",
                    ))
                })?,
            ))),
            r2d2_redis::redis::Value::Bulk(v) => Ok(Wrapper(rhai::Dynamic::from_array(
                v.iter()
                    .map(|value| Self::from_redis_value(value).map(|value| value.0))
                    .collect::<Result<rhai::Array, r2d2_redis::redis::RedisError>>()?,
            ))),
            r2d2_redis::redis::Value::Status(v) => Ok(Wrapper(rhai::Dynamic::from(v.clone()))),
            r2d2_redis::redis::Value::Okay => Ok(Wrapper(rhai::Dynamic::from_map(
                rhai::Map::from_iter([("okay".into(), rhai::Dynamic::UNIT)]),
            ))),
        }
    }
}

impl RedisConnector {
    pub fn set(&self, key: &str, value: Dynamic) -> Result<String, Box<rhai::EvalAltResult>> {
        let mut client = self.pool.get();
        match client {
            Ok(ref mut client) => {
                let result: String = client
                    .set(key, Wrapper(value))
                    .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
                Ok(result)
            }
            Err(e) => {
                Err(e).map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?
            }
        }
    }

    pub fn get(&self, key: &str) -> Result<rhai::Dynamic, Box<rhai::EvalAltResult>> {
        let mut client = self.pool.get();
        match client {
            Ok(ref mut client) => {
                let result: Option<Wrapper> = client
                    .get(key)
                    .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
                match result {
                    Some(result) => Ok(result.0),
                    None => Ok(Dynamic::UNIT),
                }
            }
            Err(e) => {
                Err(e).map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?
            }
        }
    }

    pub fn keys(&self, key: &str) -> Result<rhai::Dynamic, Box<rhai::EvalAltResult>> {
        let mut client = self.pool.get();
        match client {
            Ok(ref mut client) => {
                let result: Vec<String> = client
                    .keys(key)
                    .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
                Ok(result.into())
            }
            Err(e) => {
                Err(e).map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?
            }
        }
    }

    pub fn delete(&self, key: &str) -> Result<(), Box<rhai::EvalAltResult>> {
        let mut client = self.pool.get();
        match client {
            Ok(ref mut client) => {
                client
                    .del(key)
                    .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
                Ok(())
            }
            Err(e) => {
                Err(e).map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?
            }
        }
    }

    pub fn append(&self, key: &str, value: Dynamic) -> Result<Dynamic, Box<rhai::EvalAltResult>> {
        let mut client = self.pool.get();
        match client {
            Ok(ref mut client) => {
                let result: String = client
                    .append(key, Wrapper(value))
                    .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
                Ok(result.into())
            }
            Err(e) => {
                Err(e).map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?
            }
        }
    }

    pub fn increment(
        &self,
        key: &str,
        delta: rhai::INT,
    ) -> Result<rhai::INT, Box<rhai::EvalAltResult>> {
        let mut client = self.pool.get();
        match client {
            Ok(ref mut client) => {
                let result: rhai::INT = client
                    .incr(key, delta)
                    .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
                Ok(result)
            }
            Err(e) => {
                Err(e).map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?
            }
        }
    }

    pub fn decrement(
        &self,
        key: &str,
        delta: rhai::INT,
    ) -> Result<rhai::INT, Box<rhai::EvalAltResult>> {
        let mut client = self.pool.get();
        match client {
            Ok(ref mut client) => {
                let result: rhai::INT = client
                    .decr(key, delta)
                    .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
                Ok(result)
            }
            Err(e) => {
                Err(e).map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?
            }
        }
    }
}

/// This plugin exposes methods to open a pool of connexions to a redis database using
/// Rhai.
#[rhai::plugin::export_module]
pub mod vsmtp_plugin_redis {
    pub type Red = rhai::Shared<RedisConnector>;

    /// Open a pool of connections to a Redis database.
    ///
    /// # Args
    ///
    /// * `parameters` - a map of the following parameters:
    ///     * `url` - a string url to connect to the database.
    ///     * `timeout` - time allowed between each query to the database. (default: 30s)
    ///     * `connections` - Number of connections to open to the database. (default: 4)
    ///
    /// # Return
    ///
    /// A service used to query the database pointed by the `url` parameter.
    ///
    /// # Error
    ///
    /// * The service failed to connect to the database.
    ///
    /// # Example
    ///
    /// ```text
    /// // Import the plugin stored in the `plugins` directory.
    /// import "plugins/libvsmtp_plugin_redis" as redis;
    ///
    /// export const database = redis::connect(#{
    ///     // Connect to a database on the system.
    ///     url: "redis://localhost:6379",
    ///     timeout: "1m",
    ///     connections: 1,
    /// });
    /// ```
    #[rhai_fn(global, return_raw)]
    pub fn connect(parameters: rhai::Map) -> Result<Red, Box<rhai::EvalAltResult>> {
        let parameters = rhai::serde::from_dynamic::<RedisDatabaseParameters>(&parameters.into())?;
        let manager =
            RedisConnectionManager::new(parameters.url.clone())
                .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;

        Ok(rhai::Shared::new(RedisConnector {
            url: parameters.url,
            pool: r2d2_redis::r2d2::Pool::builder()
                .max_size(
                    u32::try_from(parameters.connections)
                        .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?,
                )
                .connection_timeout(parameters.timeout)
                .build(manager)
                .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?,
        }))
    }

    /// Set a value with its associate key into the server.
    ///
    /// # Args
    ///
    /// * `key` - The key you want to allocate with the value
    /// * `value` - The value you want to store
    ///
    /// # Return
    ///
    /// A string containing "OK" if the set was successful
    ///
    /// # Example
    ///
    /// Build a service in `services/redis.vsl`;
    ///
    /// ```text
    /// // Import the plugin stored in the `plugins` directory.
    /// import "plugins/libvsmtp_plugin_redis" as redis;
    ///
    /// export const client = redis::connect(#{
    ///     url: "redis://localhost:6379",
    ///     connections: 1,
    /// });
    /// ```
    ///
    /// Set a value during filtering.
    ///
    /// ```text
    /// import "services/redis" as srv;
    ///
    /// #{
    ///     connect: [
    ///         action "set a value in my redis server" || {
    ///             const okay = srv::client.set("my_key", "0.0.0.0");
    ///             log("info", `status is: ${okay}`);
    ///         }
    ///     ],
    /// }
    /// ```
    #[rhai_fn(global, return_raw, pure)]
    pub fn set(
        con: &mut Red,
        key: &str,
        value: Dynamic,
    ) -> Result<String, Box<rhai::EvalAltResult>> {
        con.set(key, value)
    }

    /// Get something from the server.
    ///
    /// # Args
    ///
    /// * `key` - The key you want to get the value from
    ///
    /// # Return
    ///
    /// A rhai::Dynamic with the value inside
    ///
    /// # Example
    ///
    /// Build a service in `services/redis.vsl`;
    ///
    /// ```text
    /// // Import the plugin stored in the `plugins` directory.
    /// import "plugins/libvsmtp_plugin_redis" as redis;
    ///
    /// export const client = redis::connect(#{
    ///     url: "redis://localhost:6379",
    ///     connections: 1,
    /// });
    /// ```
    ///
    /// Get the value wanted during filtering.
    ///
    /// ```text
    /// import "services/redis" as srv;
    ///
    /// #{
    ///     connect: [
    ///         action "get value from my redis server" || {
    ///             // For the sake of this example, we assume that there is a "my_key" as a key and "0.0.0.0" as its value.
    ///             const my_key = srv.get("my_key");
    ///             log("info", `my key value is: ${my_key}`);
    ///         }
    ///     ],
    /// }
    /// ```
    #[rhai_fn(global, return_raw, pure)]
    pub fn get(con: &mut Red, key: &str) -> Result<rhai::Dynamic, Box<rhai::EvalAltResult>> {
        con.get(key)
    }

    /// Get all the keys matching pattern from the server.
    ///
    /// # Args
    ///
    /// * `key` - The pattern you want to get the keys from
    ///
    /// # Return
    ///
    /// A rhai::Dynamic with the values inside
    ///
    /// # Example
    ///
    /// Build a service in `services/redis.vsl`;
    ///
    /// ```text
    /// // Import the plugin stored in the `plugins` directory.
    /// import "plugins/libvsmtp_plugin_redis" as redis;
    ///
    /// export const client = redis::connect(#{
    ///     url: "redis://localhost:6379",
    ///     connections: 1,
    /// });
    /// ```
    ///
    /// Get the values wanted during filtering.
    ///
    /// ```text
    /// import "services/redis" as srv;
    ///
    /// #{
    ///     connect: [
    ///         action "get keys from my redis server" || {
    ///             const keys = srv::client.keys("*");
    ///             for key in keys {
    ///                 log("info", `->: ${key}`);
    ///             }
    ///         }
    ///     ],
    /// }
    /// ```
    #[rhai_fn(global, return_raw, pure)]
    pub fn keys(con: &mut Red, key: &str) -> Result<rhai::Dynamic, Box<rhai::EvalAltResult>> {
        con.keys(key)
    }

    /// Delete value of the specified key.
    ///
    /// # Args
    ///
    /// * `key` - The key you want the value to be deleted
    ///
    /// # Example
    ///
    /// Build a service in `services/redis.vsl`;
    ///
    /// ```text
    /// // Import the plugin stored in the `plugins` directory.
    /// import "plugins/libvsmtp_plugin_redis" as redis;
    ///
    /// export const client = redis::connect(#{
    ///    url: "redis://localhost:6379",
    ///    connections: 1,
    /// });
    /// ```
    ///
    /// Delete the value wanted during filtering.
    ///
    /// ```text
    /// import "services/redis" as srv;
    ///
    /// #{
    ///    connect: [
    ///        action "delete value into my redis server" || {
    ///             srv::client.set("my_key", "0.0.0.0");
    ///             srv::client.delete("my_key");
    ///             // Will return nothing
    ///             const my_key = srv::client.get("my_key");
    ///             log("info", `my key value is: ${my_key}`);
    ///    }
    ///   ],
    /// }
    /// ```
    #[rhai_fn(global, return_raw, pure)]
    pub fn delete(con: &mut Red, key: &str) -> Result<(), Box<rhai::EvalAltResult>> {
        con.delete(key)
    }

    /// Append a value to a key.
    ///
    /// # Args
    ///
    /// * `key` - The key you want to append with the value
    /// * `value` - The value you want to append
    ///
    /// # Example
    ///
    /// Build a service in `services/redis.vsl`;
    ///
    /// ```text
    /// // Import the plugin stored in the `plugins` directory.
    /// import "plugins/libvsmtp_plugin_redis" as redis;
    ///
    /// export const client = redis::connect(#{
    ///    url: "redis://localhost:6379",
    ///    connections: 1,
    /// });
    /// ```
    ///
    /// Append the value wanted during filtering.
    ///
    /// ```text
    /// import "services/redis" as srv;
    ///
    /// #{
    ///     connect: [
    ///        action "append value into my redis server" || {
    ///        srv::client.set("mykey", "0.0.");
    ///        // Will get an error if the key doesn't exist
    ///        srv::client.append("mykey", "0.0");
    ///        const my_key = srv::client.get("mykey");
    ///        log("info", `my key value is: ${my_key}`);
    ///   }
    /// ],
    /// }
    #[rhai_fn(global, return_raw, pure)]
    pub fn append(
        con: &mut Red,
        key: &str,
        value: Dynamic,
    ) -> Result<Dynamic, Box<rhai::EvalAltResult>> {
        con.append(key, value)
    }

    /// Increment value of the specified key.
    ///
    /// # Args
    ///
    /// * `key` - The key you want the value to be incremented
    /// * `value` - Amount of the increment
    ///
    /// # Example
    ///
    /// Build a service in `services/redis.vsl`;
    ///
    /// ```text
    /// // Import the plugin stored in the `plugins` directory.
    /// import "plugins/libvsmtp_plugin_redis" as redis;
    ///
    /// export const client = redis::connect(#{
    ///    url: "redis://localhost:6379",
    ///    connections: 1,
    /// });
    /// ```
    ///
    /// Increment a value during filtering.
    ///
    /// ```text
    /// import "services/redis" as srv;
    ///
    /// #{
    ///    connect: [
    ///       action "increment value into my redis server" || {
    ///             srv::client.set("my_key", 1);
    ///             srv::client.increment("my_key", 21);
    ///             const my_key = srv::client.get("my_key");
    ///             // Should be 22
    ///             log("info", `my_key is now: ${my_key}`);
    ///         }
    ///    ],
    /// }
    #[rhai_fn(global, return_raw, pure)]
    pub fn increment(
        con: &mut Red,
        key: &str,
        delta: rhai::INT,
    ) -> Result<rhai::INT, Box<rhai::EvalAltResult>> {
        con.increment(key, delta)
    }

    /// Decrement value of the specified key.
    ///
    /// # Args
    ///
    /// * `key` - The key you want the value to be decremented
    /// * `value` - Amount of the decrement
    ///
    /// # Example
    ///
    /// Build a service in `services/redis.vsl`;
    ///
    /// ```text
    /// // Import the plugin stored in the `plugins` directory.
    /// import "plugins/libvsmtp_plugin_redis" as redis;
    ///
    /// export const client = redis::connect(#{
    ///     url: "redis://localhost:6379",
    ///     connections: 1,
    /// });
    /// ```
    ///
    /// Decrement a value during filtering.
    ///
    /// ```text
    /// import "services/redis" as srv;
    ///
    /// #{
    ///    connect: [
    ///       action "decrement value into my redis server" || {
    ///             srv::client.set("my_key", 23);
    ///             srv::client.decrement("my_key", 22);
    ///             const my_key = srv::client.get("my_key");
    ///             // Should be 1
    ///             log("info", `my_key is now: ${my_key}`);
    ///       }
    ///   ],
    /// }
    /// ```
    #[rhai_fn(global, return_raw, pure)]
    pub fn decrement(
        con: &mut Red,
        key: &str,
        delta: rhai::INT,
    ) -> Result<rhai::INT, Box<rhai::EvalAltResult>> {
        con.decrement(key, delta)
    }
}
