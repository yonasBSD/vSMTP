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

use mongodb::{bson::Document, sync::Client};
use rhai::plugin::*;

#[derive(Clone)]
/// A mongodb connector.
pub struct MongodbConnector {
    /// The url to the mongodb server.
    pub url: String,
    /// connection to the database
    pub client: mongodb::sync::Client,
}

pub struct MongodbDatabase {
    pub name: String,
    pub database: mongodb::sync::Database,
}

pub struct MongodbCollection {
    pub name: String,
    pub collection: mongodb::sync::Collection<Document>,
}

impl MongodbConnector {
    pub fn database(
        &self,
        name: String,
    ) -> Result<mongodb::sync::Database, Box<rhai::EvalAltResult>> {
        Ok(self.client.database(&name))
    }
}

impl MongodbDatabase {
    pub fn collection(
        &self,
        name: String,
    ) -> Result<mongodb::sync::Collection<Document>, Box<rhai::EvalAltResult>> {
        Ok(self.database.collection(&name))
    }
}

impl MongodbCollection {
    pub fn insert_one(&self, document: rhai::Dynamic) -> Result<String, Box<rhai::EvalAltResult>> {
        let document = rhai::serde::from_dynamic::<Document>(&document)?;
        let result = self
            .collection
            .insert_one(document, None)
            .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
        let object_id = result.inserted_id.as_object_id();
        match object_id {
            Some(obj_id) => Ok(obj_id.to_hex()),
            None => Ok(String::from("")),
        }
    }

    pub fn insert_many(
        &self,
        documents: rhai::Array,
    ) -> Result<Vec<String>, Box<rhai::EvalAltResult>> {
        let v = documents.to_vec();
        let v: Vec<Document> = v
            .iter()
            .map(|x| rhai::serde::from_dynamic::<Document>(x).unwrap_or_default())
            .collect();
        let results = self
            .collection
            .insert_many(v, None)
            .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
        let object_ids = results
            .inserted_ids
            .into_iter()
            .map(|x| x.1.as_object_id().unwrap_or_default().to_hex())
            .collect::<Vec<String>>();
        Ok(object_ids)
    }

    pub fn create_index(&self, index: rhai::Dynamic) -> Result<String, Box<rhai::EvalAltResult>> {
        let index = rhai::serde::from_dynamic::<mongodb::IndexModel>(&index)?;
        let result = self
            .collection
            .create_index(index, None)
            .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
        Ok(result.index_name)
    }

    pub fn create_indexes(
        &self,
        indexes: rhai::Array,
    ) -> Result<Vec<String>, Box<rhai::EvalAltResult>> {
        let v = indexes.to_vec();
        let v: Vec<mongodb::IndexModel> = v
            .iter()
            .map(|x| rhai::serde::from_dynamic::<mongodb::IndexModel>(x).unwrap_or_default())
            .collect();
        let results = self
            .collection
            .create_indexes(v, None)
            .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
        Ok(results.index_names)
    }

    pub fn delete_one(&self, query: rhai::Dynamic) -> Result<u64, Box<rhai::EvalAltResult>> {
        let filter = rhai::serde::from_dynamic::<Document>(&query)?;
        let result = self
            .collection
            .delete_one(filter, None)
            .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
        Ok(result.deleted_count)
    }

    pub fn delete_many(&self, query: rhai::Dynamic) -> Result<u64, Box<rhai::EvalAltResult>> {
        let filter = rhai::serde::from_dynamic::<Document>(&query)?;
        let result = self
            .collection
            .delete_many(filter, None)
            .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
        Ok(result.deleted_count)
    }

    pub fn find_one(
        &self,
        query: rhai::Dynamic,
    ) -> Result<rhai::Dynamic, Box<rhai::EvalAltResult>> {
        let filter = rhai::serde::from_dynamic::<Document>(&query)?;
        let result = self
            .collection
            .find_one(filter, None)
            .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
        match result {
            Some(doc) => rhai::serde::to_dynamic(doc),
            None => Ok(rhai::Dynamic::UNIT),
        }
    }

    pub fn find_one_and_update(
        &self,
        filter: rhai::Dynamic,
        update: rhai::Dynamic,
    ) -> Result<rhai::Dynamic, Box<rhai::EvalAltResult>> {
        let update = rhai::serde::from_dynamic::<Document>(&update)?;
        let filter = rhai::serde::from_dynamic::<Document>(&filter)?;
        let result = self
            .collection
            .find_one_and_update(filter, update, None)
            .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
        match result {
            Some(doc) => rhai::serde::to_dynamic(doc),
            None => Ok(rhai::Dynamic::UNIT),
        }
    }

    pub fn find_one_and_replace(
        &self,
        filter: rhai::Dynamic,
        replacement: rhai::Dynamic,
    ) -> Result<rhai::Dynamic, Box<rhai::EvalAltResult>> {
        let replacement = rhai::serde::from_dynamic::<Document>(&replacement)?;
        let filter = rhai::serde::from_dynamic::<Document>(&filter)?;
        let result = self
            .collection
            .find_one_and_replace(filter, replacement, None)
            .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
        match result {
            Some(doc) => rhai::serde::to_dynamic(doc),
            None => Ok(rhai::Dynamic::UNIT),
        }
    }

    pub fn find_one_and_delete(
        &self,
        filter: rhai::Dynamic,
    ) -> Result<rhai::Dynamic, Box<rhai::EvalAltResult>> {
        let filter = rhai::serde::from_dynamic::<Document>(&filter)?;
        let result = self
            .collection
            .find_one_and_delete(filter, None)
            .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
        match result {
            Some(doc) => rhai::serde::to_dynamic(doc),
            None => Ok(rhai::Dynamic::UNIT),
        }
    }

    pub fn update_one(
        &self,
        filter: rhai::Dynamic,
        update: rhai::Dynamic,
    ) -> Result<rhai::Map, Box<rhai::EvalAltResult>> {
        let update = rhai::serde::from_dynamic::<Document>(&update)?;
        let filter = rhai::serde::from_dynamic::<Document>(&filter)?;
        let result = self
            .collection
            .update_one(filter, update, None)
            .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
        let mut map = rhai::Map::new();
        map.insert(
            "matched_count".into(),
            rhai::Dynamic::from(result.matched_count),
        );
        map.insert(
            "modified_count".into(),
            rhai::Dynamic::from(result.modified_count),
        );
        let upsert = match result.upserted_id {
            Some(v) => rhai::serde::to_dynamic(v)
                .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?,
            None => rhai::Dynamic::UNIT,
        };
        map.insert("upserted_id".into(), upsert);
        Ok(map)
    }

    pub fn update_many(
        &self,
        filter: rhai::Dynamic,
        update: rhai::Dynamic,
    ) -> Result<rhai::Map, Box<rhai::EvalAltResult>> {
        let update = rhai::serde::from_dynamic::<Document>(&update)?;
        let filter = rhai::serde::from_dynamic::<Document>(&filter)?;
        let result = self
            .collection
            .update_many(filter, update, None)
            .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
        let mut map = rhai::Map::new();
        map.insert(
            "matched_count".into(),
            rhai::Dynamic::from(result.matched_count),
        );
        map.insert(
            "modified_count".into(),
            rhai::Dynamic::from(result.modified_count),
        );
        let upsert = match result.upserted_id {
            Some(v) => rhai::serde::to_dynamic(v)
                .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?,
            None => rhai::Dynamic::UNIT,
        };
        map.insert("upserted_id".into(), upsert);
        Ok(map)
    }

    pub fn drop_index(&self, name: String) -> Result<(), Box<rhai::EvalAltResult>> {
        self.collection
            .drop_index(name, None)
            .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
        Ok(())
    }

    pub fn drop_indexes(&self) -> Result<(), Box<rhai::EvalAltResult>> {
        self.collection
            .drop_indexes(None)
            .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
        Ok(())
    }
}

/// This plugin exposes methods to open a pool of connexions to a mongodb database using
/// Rhai.
#[rhai::plugin::export_module]
pub mod vsmtp_plugin_mongodb {
    pub type Mongo = rhai::Shared<MongodbConnector>;
    pub type Database = rhai::Shared<MongodbDatabase>;
    pub type Collection = rhai::Shared<MongodbCollection>;

    /// Open a pool of connections to a Mongodb database.
    ///
    /// # Args
    ///
    /// * `url` - a string url to connect to the database. Make sure to put the username and password in the url
    ///
    /// # Return
    ///
    /// A service used to query the database pointed by the `url` parameter
    ///
    /// # Error
    ///
    /// * The service failed to connect to the database.
    ///
    /// # Example
    ///
    /// ```text
    /// // Import the plugin stored in the `plugins` directory.
    /// import "plugins/libvsmtp_plugin_mongodb" as mongo;
    ///
    /// export const client = mongo::connect("mongodb://admin:pass@localhost:27017");
    /// ```
    #[rhai_fn(global, return_raw)]
    pub fn connect(url: rhai::Dynamic) -> Result<Mongo, Box<rhai::EvalAltResult>> {
        Ok(rhai::Shared::new(MongodbConnector {
            url: url.to_string(),
            client: Client::with_uri_str(url.to_string())
                .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?,
        }))
    }

    /// Gets a handle to a database specified by name.
    ///
    /// # Args
    ///
    /// * `name` - the name of the database
    ///
    /// # Return
    ///
    /// A handle to the database specified by name
    ///
    /// # Example
    ///
    /// Build a service in `services/mongodb.vsl`;
    ///
    /// ```text
    /// // Import the plugin stored in the `plugins` directory.
    /// import "plugins/libvsmtp_plugin_mongodb" as mongo;
    ///
    /// export const client = mongo::connect("mongodb://admin:pass@localhost:27017");
    /// ```
    ///
    /// Get the handle for the database
    ///
    ///```text
    /// import "services/mongo" as srv;
    ///
    ///  #{
    ///     connect: [
    ///         action "get a handle from my db" || {
    ///             let database = srv::client.database("my_database");
    ///         }
    ///     ],
    /// }
    /// ```
    #[rhai_fn(global, return_raw, pure)]
    pub fn database(
        con: &mut Mongo,
        name: rhai::Dynamic,
    ) -> Result<Database, Box<rhai::EvalAltResult>> {
        Ok(rhai::Shared::new(MongodbDatabase {
            name: name.to_string(),
            database: con
                .database(name.to_string())
                .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?,
        }))
    }

    /// Gets a handle to a collection specified by name.
    ///
    /// # Args
    ///
    /// * `name` - the name of the collection
    ///
    /// # Return
    ///
    /// A handle to the collection specified by name
    ///
    /// # Example
    ///
    /// Build a service in `services/mongodb.vsl`;
    ///
    /// ```text
    /// // Import the plugin stored in the `plugins` directory.
    /// import "plugins/libvsmtp_plugin_mongodb" as mongo;
    ///
    /// export const client = mongo::connect("mongodb://admin:pass@localhost:27017");
    /// ```
    ///
    /// Get the handle for the collection
    ///
    ///```text
    /// import "services/mongo" as srv;
    ///
    ///  #{
    ///     connect: [
    ///         action "get a handle from my collection" || {
    ///             let database = srv::client.database("my_database");
    ///             let collection = database.collection("greylist");
    ///         }
    ///     ],
    /// }
    /// ```
    #[rhai_fn(global, return_raw, pure)]
    pub fn collection(
        con: &mut Database,
        name: rhai::Dynamic,
    ) -> Result<Collection, Box<rhai::EvalAltResult>> {
        Ok(rhai::Shared::new(MongodbCollection {
            name: name.to_string(),
            collection: con
                .collection(name.to_string())
                .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?,
        }))
    }

    /// Insert a value into a collection.
    ///
    /// # Args
    ///
    /// * `document` - the document to be inserted
    ///
    /// # Return
    ///
    /// The id of the inserted document
    ///
    /// # Example
    ///
    /// Build a service in `services/mongodb.vsl`;
    ///
    /// ```text
    /// // Import the plugin stored in the `plugins` directory.
    /// import "plugins/libvsmtp_plugin_mongodb" as mongo;
    ///
    /// export const client = mongo::connect("mongodb://admin:pass@localhost:27017");
    /// ```
    ///
    /// Insert the document thanks to a map
    ///
    ///```text
    /// import "services/mongo" as srv;
    ///
    ///  #{
    ///     connect: [
    ///         action "insert one values in my collection" || {
    ///             let database = srv::client.database("my_database");
    ///             let collection = database.collection("greylist");
    ///             let id = collection.insert_one(#{
    ///                 "email": "john.doe@example.com",
    ///                 "name": "John"
    ///             });
    ///             log("info", `the id is: ${id}`);
    ///         }
    ///     ],
    /// }
    /// ```
    #[rhai_fn(global, return_raw, pure)]
    pub fn insert_one(
        con: &mut Collection,
        document: rhai::Dynamic,
    ) -> Result<rhai::Dynamic, Box<rhai::EvalAltResult>> {
        let result = con
            .insert_one(document)
            .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
        Ok(result.into())
    }

    /// Insert values into a collection.
    ///
    /// # Args
    ///
    /// * `documents` - the documents to be inserted
    ///
    /// # Return
    ///
    /// Ids of the inserted documents
    ///
    /// # Example
    ///
    /// Build a service in `services/mongodb.vsl`;
    ///
    /// ```text
    /// // Import the plugin stored in the `plugins` directory.
    /// import "plugins/libvsmtp_plugin_mongodb" as mongo;
    ///
    /// export const client = mongo::connect("mongodb://admin:pass@localhost:27017");
    /// ```
    ///
    /// Insert the documents thanks to a rhai array
    ///
    ///```text
    /// import "services/mongo" as srv;
    ///
    ///  #{
    ///     connect: [
    ///         action "insert many values in my collection" || {
    ///             let database = srv::client.database("my_database");
    ///             let collection = database.collection("greylist");
    ///             const ids = collection.insert_many([
    ///                 #{
    ///                     "email": "john.doe@example.com",
    ///                     "name": "John"
    ///                 },
    ///                 #{
    ///                     "email": "jenny.doe@example.com"
    ///                     "name": "Jenny"
    ///                 }
    ///             ]);
    ///             for id in ids {
    ///                 log("info", ` -> ${id}`);
    ///             }
    ///         }
    ///     ],
    /// }
    /// ```
    #[rhai_fn(global, return_raw, pure)]
    pub fn insert_many(
        con: &mut Collection,
        documents: rhai::Array,
    ) -> Result<rhai::Dynamic, Box<rhai::EvalAltResult>> {
        let results = con
            .insert_many(documents)
            .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
        Ok(results.into())
    }

    /// Create an index into a collection.
    ///
    /// # Args
    ///
    /// * `index` - the index to be inserted
    ///
    /// # Return
    ///
    /// The name of the created index
    ///
    /// # Example
    ///
    /// Build a service in `services/mongodb.vsl`;
    ///
    /// ```text
    /// // Import the plugin stored in the `plugins` directory.
    /// import "plugins/libvsmtp_plugin_mongodb" as mongo;
    ///
    /// export const client = mongo::connect("mongodb://admin:pass@localhost:27017");
    /// ```
    ///
    /// Create an index thanks to a rhai map
    ///
    ///```text
    /// import "services/mongo" as srv;
    ///
    ///  #{
    ///     connect: [
    ///         action "create index into my collection" || {
    ///             let database = srv::client.database("my_database");
    ///             let collection = database.collection("greylist");
    ///             let name = collection.create_index(#{
    ///                 "key": #{
    ///                     "email": -1,
    ///                     "name": -1
    ///                 }
    ///             });
    ///             log("info", `the name of the index is: ${name}`);
    ///         }
    ///     ],
    /// }
    /// ```
    #[rhai_fn(global, return_raw, pure)]
    pub fn create_index(
        con: &mut Collection,
        index: rhai::Dynamic,
    ) -> Result<rhai::Dynamic, Box<rhai::EvalAltResult>> {
        let result = con
            .create_index(index)
            .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
        Ok(result.into())
    }

    /// Create indexes into a collection.
    ///
    /// # Args
    ///
    /// * `indexes` - the indexes to be inserted
    ///
    /// # Return
    ///
    /// Names of the created indexes
    ///
    /// # Example
    ///
    /// Build a service in `services/mongodb.vsl`;
    ///
    /// ```text
    /// // Import the plugin stored in the `plugins` directory.
    /// import "plugins/libvsmtp_plugin_mongodb" as mongo;
    ///
    /// export const client = mongo::connect("mongodb://admin:pass@localhost:27017");
    /// ```
    ///
    /// Create indexes thanks to a rhai array
    ///
    ///```text
    /// import "services/mongo" as srv;
    ///
    ///  #{
    ///     connect: [
    ///         action "create indexes into my collection" || {
    ///             let database = srv::client.database("my_database");
    ///             let collection = database.collection("greylist");
    ///             const names = collection.create_indexes([
    ///                 #{
    ///                     "key": #{
    ///                         "email": -1
    ///                     }
    ///                 },
    ///                 #{
    ///                     "key": #{
    ///                         "name": -1
    ///                     }
    ///                 }
    ///             ]);
    ///             for name in names {
    ///                 log("info", ` -> ${name}`);
    ///             }
    ///         }
    ///     ],
    /// }
    /// ```
    #[rhai_fn(global, return_raw, pure)]
    pub fn create_indexes(
        con: &mut Collection,
        indexes: rhai::Array,
    ) -> Result<rhai::Dynamic, Box<rhai::EvalAltResult>> {
        let results = con
            .create_indexes(indexes)
            .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
        Ok(results.into())
    }

    /// Deletes a document from the collection.
    ///
    /// # Args
    ///
    /// * `query` - the query to use when deleting the document
    ///
    /// # Return
    ///
    /// The amount of deleted document
    ///
    /// # Example
    ///
    /// Build a service in `services/mongodb.vsl`;
    ///
    /// ```text
    /// // Import the plugin stored in the `plugins` directory.
    /// import "plugins/libvsmtp_plugin_mongodb" as mongo;
    ///
    /// export const client = mongo::connect("mongodb://admin:pass@localhost:27017");
    /// ```
    ///
    /// Delete the selected document
    ///
    ///```text
    /// import "services/mongo" as srv;
    ///
    ///  #{
    ///     connect: [
    ///         action "delete one document in my collection" || {
    ///             let database = srv::client.database("my_database");
    ///             let collection = database.collection("greylist");
    ///             collection.insert_many([
    ///                 #{
    ///                     "email": "john.doe@example.com",
    ///                     "name": "John"
    ///                 },
    ///                 #{
    ///                     "email": "jenny.doe@example.com"
    ///                     "name": "Jenny"
    ///                 }
    ///             ]);
    ///             let count = collection.delete_one(#{
    ///                 "email": "john.doe@example.com",
    ///                 "name": "John"
    ///             });
    ///             log("info", `the amount of deleted indexes is: ${count}`);
    ///         }
    ///     ],
    /// }
    /// ```
    #[rhai_fn(global, return_raw, pure)]
    pub fn delete_one(
        con: &mut Collection,
        query: rhai::Dynamic,
    ) -> Result<rhai::INT, Box<rhai::EvalAltResult>> {
        let result = con
            .delete_one(query)
            .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
        let count = rhai::INT::try_from(result)
            .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
        Ok(count)
    }

    /// Deletes all documents matching query from the collection.
    ///
    /// # Args
    ///
    /// * `query` - the query to use when deleting the document
    ///
    /// # Return
    ///
    /// The amount of deleted document
    ///
    /// # Example
    ///
    /// Build a service in `services/mongodb.vsl`;
    ///
    /// ```text
    /// // Import the plugin stored in the `plugins` directory.
    /// import "plugins/libvsmtp_plugin_mongodb" as mongo;
    ///
    /// export const client = mongo::connect("mongodb://admin:pass@localhost:27017");
    /// ```
    ///
    /// Delete the selected document
    ///
    ///```text
    /// import "services/mongo" as srv;
    ///
    ///  #{
    ///     connect: [
    ///         action "delete many document in my collection" || {
    ///             let database = srv::client.database("my_database");
    ///             let collection = database.collection("greylist");
    ///             collection.insert_many([
    ///                 #{
    ///                     "email": "john.doe@example.com",
    ///                     "name": "John"
    ///                 },
    ///                 #{
    ///                     "email": "john.does@example.com"
    ///                     "name": "John"
    ///                 }
    ///             ]);
    ///             let count = collection.delete_many(#{
    ///                 "name": "John"
    ///             });
    ///             log("info", `the amount of deleted indexes is: ${count}`);
    ///         }
    ///     ],
    /// }
    /// ```
    #[rhai_fn(global, return_raw, pure)]
    pub fn delete_many(
        con: &mut Collection,
        query: rhai::Dynamic,
    ) -> Result<rhai::INT, Box<rhai::EvalAltResult>> {
        let result = con
            .delete_many(query)
            .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
        let count = rhai::INT::try_from(result)
            .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
        Ok(count)
    }

    /// Finds a single document in the collection matching filter.
    ///
    /// # Args
    ///
    /// * `filter` - a document containing query operators
    ///
    /// # Return
    ///
    /// The found document as a rhai map
    ///
    /// # Example
    ///
    /// Build a service in `services/mongodb.vsl`;
    ///
    /// ```text
    /// // Import the plugin stored in the `plugins` directory.
    /// import "plugins/libvsmtp_plugin_mongodb" as mongo;
    ///
    /// export const client = mongo::connect("mongodb://admin:pass@localhost:27017");
    /// ```
    ///
    /// Find the first document matching query
    ///
    ///```text
    /// import "services/mongo" as srv;
    ///
    ///  #{
    ///     connect: [
    ///         action "find a document in my collection" || {
    ///             let database = srv::client.database("my_database");
    ///             let collection = database.collection("greylist");
    ///             collection.insert_many([
    ///                 #{
    ///                     "email": "john.doe@example.com",
    ///                     "name": "John"
    ///                 },
    ///                 #{
    ///                     "email": "jenny.doe@example.com"
    ///                     "name": "Jenny"
    ///                 }
    ///             ]);
    ///             let user = collection.find_one(#{
    ///                 "name": "John"
    ///             });
    ///             log("info", `the email is: ${user.email}`);
    ///         }
    ///     ],
    /// }
    /// ```
    #[rhai_fn(global, return_raw, pure)]
    pub fn find_one(
        con: &mut Collection,
        filter: rhai::Dynamic,
    ) -> Result<rhai::Dynamic, Box<rhai::EvalAltResult>> {
        let result = con
            .find_one(filter)
            .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
        Ok(result)
    }

    /// Finds up to one document in the collection matching filter and updates it.
    ///
    /// # Args
    ///
    /// * `filter` - a document containing query operators
    /// * `update` - a document containing update operators
    ///
    /// # Return
    ///
    /// The found document as a rhai map
    ///
    /// # Example
    ///
    /// Build a service in `services/mongodb.vsl`;
    ///
    /// ```text
    /// // Import the plugin stored in the `plugins` directory.
    /// import "plugins/libvsmtp_plugin_mongodb" as mongo;
    ///
    /// export const client = mongo::connect("mongodb://admin:pass@localhost:27017");
    /// ```
    ///
    /// Find the first document matching query
    ///
    ///```text
    /// import "services/mongo" as srv;
    ///
    ///  #{
    ///     connect: [
    ///         action "find a document in my collection and update" || {
    ///             let database = srv::client.database("my_database");
    ///             let collection = database.collection("greylist");
    ///             collection.insert_many([
    ///                 #{
    ///                     "email": "john.doe@example.com",
    ///                     "name": "John"
    ///                 },
    ///                 #{
    ///                     "email": "jenny.doe@example.com"
    ///                     "name": "Jenny"
    ///                 }
    ///             ]);
    ///             let user = collection.find_one_and_update(
    ///                 #{
    ///                     "name": "John"
    ///                 },
    ///                 #{
    ///                     "$set": #{
    ///                         "name": "Johnny",
    ///                         "ip": "0.0.0.0"
    ///                     }
    ///                 }
    ///             );
    ///             log("info", `the id is: ${user._id["$oid"]}`);
    ///         }
    ///     ],
    /// }
    /// ```
    #[rhai_fn(global, return_raw, pure)]
    pub fn find_one_and_update(
        con: &mut Collection,
        filter: rhai::Dynamic,
        update: rhai::Dynamic,
    ) -> Result<rhai::Dynamic, Box<rhai::EvalAltResult>> {
        let result = con
            .find_one_and_update(filter, update)
            .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
        Ok(result)
    }

    /// Finds up to one document in the collection matching filter and replaces it.
    ///
    /// # Args
    ///
    /// * `filter` - a document containing query operators
    /// * `replacement` - a document containing update operators
    ///
    /// # Return
    ///
    /// The found document as a rhai map
    ///
    /// # Example
    ///
    /// Build a service in `services/mongodb.vsl`;
    ///
    /// ```text
    /// // Import the plugin stored in the `plugins` directory.
    /// import "plugins/libvsmtp_plugin_mongodb" as mongo;
    ///
    /// export const client = mongo::connect("mongodb://admin:pass@localhost:27017");
    /// ```
    ///
    /// Find the first document matching query
    ///
    ///```text
    /// import "services/mongo" as srv;
    ///
    ///  #{
    ///     connect: [
    ///         action "find a document in my collection and replace" || {
    ///             let database = srv::client.database("my_database");
    ///             let collection = database.collection("greylist");
    ///             collection.insert_many([
    ///                 #{
    ///                     "email": "john.doe@example.com",
    ///                     "name": "John"
    ///                 },
    ///                 #{
    ///                     "email": "jenny.doe@example.com"
    ///                     "name": "Jenny"
    ///                 }
    ///             ]);
    ///             let user = collection.find_one_and_replace(
    ///                 #{
    ///                     "name": "John"
    ///                 },
    ///                 #{
    ///                     "name": "Johnny",
    ///                     "ip": "0.0.0.0"
    ///                 }
    ///             );
    ///             log("info", `the id is: ${user._id["$oid"]}`);
    ///         }
    ///     ],
    /// }
    /// ```
    #[rhai_fn(global, return_raw, pure)]
    pub fn find_one_and_replace(
        con: &mut Collection,
        filter: rhai::Dynamic,
        replacement: rhai::Dynamic,
    ) -> Result<rhai::Dynamic, Box<rhai::EvalAltResult>> {
        let result = con
            .find_one_and_replace(filter, replacement)
            .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
        Ok(result)
    }

    /// Finds up to one document in the collection matching filter and deletes it.
    ///
    /// # Args
    ///
    /// * `filter` - a document containing query operators
    ///
    /// # Return
    ///
    /// The deleted document as a rhai map
    ///
    /// # Example
    ///
    /// Build a service in `services/mongodb.vsl`;
    ///
    /// ```text
    /// // Import the plugin stored in the `plugins` directory.
    /// import "plugins/libvsmtp_plugin_mongodb" as mongo;
    ///
    /// export const client = mongo::connect("mongodb://admin:pass@localhost:27017");
    /// ```
    ///
    /// Find the first document matching query
    ///
    ///```text
    /// import "services/mongo" as srv;
    ///
    ///  #{
    ///     connect: [
    ///         action "find a document in my collection and delete" || {
    ///             let database = srv::client.database("my_database");
    ///             let collection = database.collection("greylist");
    ///             collection.insert_many([
    ///                 #{
    ///                     "email": "john.doe@example.com",
    ///                     "name": "John"
    ///                 },
    ///                 #{
    ///                     "email": "jenny.doe@example.com"
    ///                     "name": "Jenny"
    ///                 }
    ///             ]);
    ///             let user = collection.find_one_and_delete(#{
    ///                 "email": "john.doe@example.com"
    ///             });
    ///             log("info", `the id is: ${user._id["$oid"]}`);
    ///         }
    ///     ],
    /// }
    /// ```
    #[rhai_fn(global, return_raw, pure)]
    pub fn find_one_and_delete(
        con: &mut Collection,
        filter: rhai::Dynamic,
    ) -> Result<rhai::Dynamic, Box<rhai::EvalAltResult>> {
        let result = con
            .find_one_and_delete(filter)
            .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
        Ok(result)
    }

    /// Updates a single document in the collection according to the specified arguments.
    ///
    /// # Args
    ///
    /// * `query` - a document containing query operators
    /// * `update` - a document containing update operators
    ///
    /// # Return
    ///
    /// A rhai map with the matched_count, the modified_count and the upserted_id of the query
    ///
    /// # Example
    ///
    /// Build a service in `services/mongodb.vsl`;
    ///
    /// ```text
    /// // Import the plugin stored in the `plugins` directory.
    /// import "plugins/libvsmtp_plugin_mongodb" as mongo;
    ///
    /// export const client = mongo::connect("mongodb://admin:pass@localhost:27017");
    /// ```
    ///
    /// Update the document
    ///
    ///```text
    /// import "services/mongo" as srv;
    ///
    ///  #{
    ///     connect: [
    ///         action "update a document" || {
    ///             let database = srv::client.database("my_database");
    ///             let collection = database.collection("greylist");
    ///             collection.insert_many([
    ///                 #{
    ///                     "email": "john.doe@example.com",
    ///                     "name": "John"
    ///                 },
    ///                 #{
    ///                     "email": "jenny.doe@example.com"
    ///                     "name": "Jenny"
    ///                 }
    ///             ]);
    ///             let result = collection.update_one(
    ///                 #{
    ///                     "name": "John"
    ///                 },
    ///                 #{
    ///                     "$set": #{
    ///                         "name": "Johnny",
    ///                         "ip": "0.0.0.0",
    ///                     }
    ///                 }
    ///             );
    ///             log("info", `modified count is: ${result.modified_count.to_string()}`);
    ///         }
    ///     ],
    /// }
    /// ```
    #[rhai_fn(global, return_raw, pure)]
    pub fn update_one(
        con: &mut Collection,
        query: rhai::Dynamic,
        update: rhai::Dynamic,
    ) -> Result<rhai::Dynamic, Box<rhai::EvalAltResult>> {
        let result = con
            .update_one(query, update)
            .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
        Ok(rhai::Dynamic::from_map(result))
    }

    /// Updates many document in the collection according to the specified arguments.
    ///
    /// # Args
    ///
    /// * `query` - a document containing query operators
    /// * `update` - a document containing update operators
    ///
    /// # Return
    ///
    /// A rhai map with the matched_count, the modified_count and the upserted_id of the query
    ///
    /// # Example
    ///
    /// Build a service in `services/mongodb.vsl`;
    ///
    /// ```text
    /// // Import the plugin stored in the `plugins` directory.
    /// import "plugins/libvsmtp_plugin_mongodb" as mongo;
    ///
    /// export const client = mongo::connect("mongodb://admin:pass@localhost:27017");
    /// ```
    ///
    /// Update the documents
    ///
    ///```text
    /// import "services/mongo" as srv;
    ///
    ///  #{
    ///     connect: [
    ///         action "update many documents" || {
    ///             let database = srv::client.database("my_database");
    ///             let collection = database.collection("greylist");
    ///             collection.insert_many([
    ///                 #{
    ///                     "email": "john.doe@example.com",
    ///                     "name": "John"
    ///                 },
    ///                 #{
    ///                     "email": "john.does@example.com"
    ///                     "name": "John"
    ///                 }
    ///             ]);
    ///             let result = collection.update_one(
    ///                 #{
    ///                     "name": "John"
    ///                 },
    ///                 #{
    ///                     "$set": #{
    ///                         "name": "Johnny",
    ///                         "ip": "0.0.0.0",
    ///                     }
    ///                 }
    ///             );
    ///             log("info", `modified count is: ${result.modified_count.to_string()}`);
    ///         }
    ///     ],
    /// }
    /// ```
    #[rhai_fn(global, return_raw, pure)]
    pub fn update_many(
        con: &mut Collection,
        query: rhai::Dynamic,
        update: rhai::Dynamic,
    ) -> Result<rhai::Dynamic, Box<rhai::EvalAltResult>> {
        let result = con
            .update_many(query, update)
            .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
        Ok(rhai::Dynamic::from_map(result))
    }

    /// Drops the index specified by name from this collection.
    ///
    /// # Args
    ///
    /// * `name` - the name of the index to drop
    ///
    /// # Example
    ///
    /// Build a service in `services/mongodb.vsl`;
    ///
    /// ```text
    /// // Import the plugin stored in the `plugins` directory.
    /// import "plugins/libvsmtp_plugin_mongodb" as mongo;
    ///
    /// export const client = mongo::connect("mongodb://admin:pass@localhost:27017");
    /// ```
    ///
    /// Drop the specified index
    ///
    ///```text
    /// import "services/mongo" as srv;
    ///
    ///  #{
    ///     connect: [
    ///         action "drop index in my collection" || {
    ///             let database = srv::client.database("my_database");
    ///             let collection = database.collection("greylist");
    ///             collection.create_index(#{
    ///                 "key": #{
    ///                     "email": -1,
    ///                     "name": -1
    ///                 }
    ///             });
    ///             collection.drop_index("email_-1_name_-1");
    ///         }
    ///     ],
    /// }
    /// ```
    #[rhai_fn(global, return_raw, pure)]
    pub fn drop_index(
        con: &mut Collection,
        name: rhai::Dynamic,
    ) -> Result<(), Box<rhai::EvalAltResult>> {
        con.drop_index(name.to_string())
            .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
        Ok(())
    }

    /// Drops all indexes associated with this collection.
    ///
    /// # Example
    ///
    /// Build a service in `services/mongodb.vsl`;
    ///
    /// ```text
    /// // Import the plugin stored in the `plugins` directory.
    /// import "plugins/libvsmtp_plugin_mongodb" as mongo;
    ///
    /// export const client = mongo::connect("mongodb://admin:pass@localhost:27017");
    /// ```
    ///
    /// Drop all indexes
    ///
    ///```text
    /// import "services/mongo" as srv;
    ///
    ///  #{
    ///     connect: [
    ///         action "drop all indexes in my collection" || {
    ///             let database = srv::client.database("my_database");
    ///             let collection = database.collection("greylist");
    ///             collection.create_index(#{
    ///                 "key": #{
    ///                     "email": -1,
    ///                     "name": -1
    ///                 }
    ///             });
    ///             collection.drop_indexes();
    ///         }
    ///     ],
    /// }
    /// ```
    #[rhai_fn(global, return_raw, pure)]
    pub fn drop_indexes(con: &mut Collection) -> Result<(), Box<rhai::EvalAltResult>> {
        con.drop_indexes()
            .map_err::<Box<rhai::EvalAltResult>, _>(|err| err.to_string().into())?;
        Ok(())
    }
}
