use serde_json::Value;

use redismodule::{Context, RedisError, RedisResult, RedisValue};
use redismodule::{NextArg, REDIS_OK};

use redisearch_api::{Document, FieldType};

use crate::error::Error;
use crate::redisjson::{Format, RedisJSON};
use crate::schema::Schema;
use crate::REDIS_JSON_TYPE;

pub mod schema_map {
    use crate::schema::Schema;
    use std::collections::HashMap;

    type SchemaMap = HashMap<String, Schema>;

    /// We keep a static map, since it needs to be accessed from multiple Redis module commands
    /// and there is no other obvious way to implement this (such as a "user data" pointer maintained
    /// for us by Redis).
    ///
    /// The init function should be called only once.
    /// Unwrapping the Option is thus safe afterwards.
    ///
    /// Since we have only one thread at the moment, getting a (mutable) reference to the map
    /// is also safe. If we add threads, we can simply wrap the map: `Option<Mutex<SchemaMap>`.
    ///
    static mut SCHEMA_MAP: Option<SchemaMap> = None;

    pub fn init() {
        let map = HashMap::new();
        unsafe {
            SCHEMA_MAP = Some(map);
        }
    }

    pub fn as_ref() -> &'static SchemaMap {
        unsafe { SCHEMA_MAP.as_ref() }.unwrap()
    }

    pub fn as_mut() -> &'static mut SchemaMap {
        unsafe { SCHEMA_MAP.as_mut() }.unwrap()
    }
}

///////////////////////////

fn add_field(index_name: &str, field_name: &str, path: &str) -> RedisResult {
    let map = schema_map::as_mut();

    let schema = if let Some(stored_schema) = map.get_mut(index_name) {
        stored_schema
    } else {
        let new_schema = Schema::new(&index_name);
        map.insert(index_name.to_owned(), new_schema);
        map.get_mut(index_name).unwrap()
    };

    if schema.fields.contains_key(field_name) {
        Err("Field already exists".into())
    } else {
        schema.index.create_field(field_name);
        schema.fields.insert(field_name.to_owned(), path.to_owned());
        REDIS_OK
    }
}

pub fn add_document(key: &str, index_name: &str, doc: &RedisJSON) -> RedisResult {
    // TODO: Index the document with RediSearch:
    // 1. Determine the index to use (how?)
    // 2. Get the fields from the index along with their associated paths
    // 3. Build a RS Document and populate the fields from our doc
    // 4. Add the Document to the index

    let map = schema_map::as_ref();

    map.get(index_name)
        .ok_or("ERR no such index".into())
        .and_then(|schema| {
            let rsdoc = create_document(key, schema, doc)?;
            schema.index.add_document(&rsdoc)?;
            REDIS_OK
        })
}

fn create_document(key: &str, schema: &Schema, doc: &RedisJSON) -> Result<Document, Error> {
    let fields = &schema.fields;

    let score = 1.0;
    let rsdoc = Document::create(key, score);

    for (field_name, path) in fields {
        let value = doc.get_doc(&path)?;

        match value {
            Value::String(v) => rsdoc.add_field(field_name, &v, FieldType::FULLTEXT),
            Value::Number(v) => rsdoc.add_field(field_name, &v.to_string(), FieldType::NUMERIC),
            Value::Bool(v) => rsdoc.add_field(field_name, &v.to_string(), FieldType::TAG),
            _ => {}
        }
    }

    Ok(rsdoc)
}

// JSON.INDEX ADD <index> <field> <path>
// JSON.INDEX DEL <index> <field>
// JSON.INDEX INFO <index> <field>
pub fn index<I>(_ctx: &Context, args: I) -> RedisResult
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter().skip(1);

    let subcommand = args.next_string()?;
    let index_name = args.next_string()?;
    let field_name = args.next_string()?;

    match subcommand.to_uppercase().as_str() {
        "ADD" => {
            let path = args.next_string()?;
            add_field(&index_name, &field_name, &path)
        }
        //"DEL" => {}
        //"INFO" => {}
        _ => Err("ERR unknown subcommand - try `JSON.INDEX HELP`".into()),
    }
}

// JSON.QGET <index> <query> <path>
pub fn qget<I>(ctx: &Context, args: I) -> RedisResult
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter().skip(1);

    let index_name = args.next_string()?;
    let query = args.next_string()?;
    let path = args.next().unwrap_or("$".to_string());

    let map = schema_map::as_ref();

    map.get(&index_name)
        .ok_or("ERR no such index".into())
        .map(|schema| &schema.index)
        .and_then(|index| {
            let results: Result<Vec<_>, RedisError> = index
                .search(&query)?
                .map(|key| {
                    let key = ctx.open_key_writable(&key);
                    let value = match key.get_value::<RedisJSON>(&REDIS_JSON_TYPE)? {
                        Some(doc) => doc.to_string(&path, Format::JSON)?.into(),
                        None => RedisValue::None,
                    };
                    Ok(value)
                })
                .collect();

            Ok(results?.into())
        })
}
