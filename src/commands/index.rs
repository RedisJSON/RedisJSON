use std::thread;

use serde_json::{Map, Value};

use redis_module::{Context, NextArg, RedisError, RedisResult, RedisValue, REDIS_OK};

use redisearch_api::{Document, FieldType, TagOptions};

use crate::error::Error;
use crate::redisjson::{Format, RedisJSON};
use crate::schema::Schema;
use crate::REDIS_JSON_TYPE;
use std::collections::HashSet;

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

pub fn add_field(index_name: &str, field_name: &str, path: &str) -> RedisResult {
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
        schema
            .index
            .create_field(field_name, 1.0, TagOptions::default());
        schema.fields.insert(field_name.to_owned(), path.to_owned());
        REDIS_OK
    }
}

fn del_schema(index_name: &str) -> RedisResult {
    match schema_map::as_mut().remove(index_name) {
        Some(_) => REDIS_OK,
        None => Err("Index not found".into()),
    }
}

pub fn clear_schema() {
    schema_map::as_mut().clear();
}

pub fn add_document(key: &str, index_name: &str, doc: &RedisJSON) -> RedisResult {
    // TODO: Index the document with RediSearch:
    // 1. Determine the index to use (how?)
    // 2. Get the fields from the index along with their associated paths
    // 3. Build a RS Document and populate the fields from our doc
    // 4. Add the Document to the index

    let map = schema_map::as_ref();

    if let Some(schema) = map.get(index_name) {
        let rsdoc = create_document(key, schema, doc)?;
        schema.index.add_document(&rsdoc)?;
    }
    REDIS_OK
}

pub fn remove_document(key: &str, index_name: &str) -> RedisResult {
    let map = schema_map::as_ref();

    if let Some(schema) = map.get(index_name) {
        schema.index.del_document(&key)?;
    }
    REDIS_OK
}

fn create_document(key: &str, schema: &Schema, doc: &RedisJSON) -> Result<Document, Error> {
    let fields = &schema.fields;

    let score = 1.0;
    let rsdoc = Document::create(key, score);

    for (field_name, path) in fields {
        let results = doc.get_values(path)?;
        if let Some(value) = results.first() {
            match value {
                Value::String(v) => rsdoc.add_field(field_name, &v, FieldType::FULLTEXT),
                Value::Number(v) => rsdoc.add_field(field_name, &v.to_string(), FieldType::NUMERIC),
                Value::Bool(v) => rsdoc.add_field(field_name, &v.to_string(), FieldType::TAG),
                _ => {}
            }
        }
    }

    Ok(rsdoc)
}

// JSON.INDEX ADD <index> <field> <path>
// JSON.INDEX DEL <index> <field>
// JSON.INDEX INFO <index> <field>
pub fn index<I>(ctx: &Context, args: I) -> RedisResult
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter().skip(1);

    let subcommand = args.next_string()?;

    match subcommand.to_uppercase().as_str() {
        "ADD" => {
            let index_name = args.next_string()?;
            let field_name = args.next_string()?;
            let path = args.next_string()?;
            add_field(&index_name, &field_name, &path)?;

            // TODO handle another "ADD" calls in prallel a running call
            thread::spawn(move || {
                let schema = if let Some(stored_schema) = schema_map::as_ref().get(&index_name) {
                    stored_schema
                } else {
                    return; // TODO handle this case
                };

                let ctx = Context::get_thread_safe_context();
                let mut cursor: u64 = 0;
                loop {
                    ctx.lock();
                    let res = scan_and_index(&ctx, &schema, cursor);
                    ctx.unlock();

                    match res {
                        Ok(c) => cursor = c,
                        Err(e) => {
                            eprintln!("Err on index {:?}", e); // TODO hadnle this better
                            return;
                        }
                    }
                    if cursor == 0 {
                        break;
                    }
                }
            });

            ctx.replicate_verbatim();
            REDIS_OK
        }
        "DEL" => {
            let index_name = args.next_string()?;
            let res = del_schema(&index_name)?;
            ctx.replicate_verbatim();
            Ok(res)
        }
        "INFO" => {
            let index_args: HashSet<String> = args.collect();
            let reply = schema_map::as_ref()
                .iter()
                .filter(|(name, _)| index_args.is_empty() || index_args.contains(*name))
                .fold(Value::Object(Map::new()), |mut indexes, (index, schema)| {
                    indexes.as_object_mut().unwrap().insert(
                        index.clone(),
                        schema.fields.iter().fold(
                            Value::Object(Map::new()),
                            |mut fields, (field, path)| {
                                fields
                                    .as_object_mut()
                                    .unwrap()
                                    .insert(field.clone(), Value::String(path.clone()));
                                fields
                            },
                        ),
                    );
                    indexes
                });
            Ok(RedisValue::BulkString(
                serde_json::to_string_pretty(&reply).unwrap(),
            ))
        }
        "FLUSH" => {
            clear_schema();
            ctx.replicate_verbatim();
            REDIS_OK
        }
        _ => Err("ERR unknown subcommand - try `JSON.INDEX HELP`".into()),
    }
}

fn scan_and_index(ctx: &Context, schema: &Schema, cursor: u64) -> Result<u64, RedisError> {
    let values = ctx.call("scan", &[&cursor.to_string()]);
    match values {
        Ok(RedisValue::Array(arr)) => match (arr.get(0), arr.get(1)) {
            (Some(RedisValue::SimpleString(next_cursor)), Some(RedisValue::Array(keys))) => {
                let cursor = next_cursor.parse().unwrap();
                let res = keys.iter().try_for_each(|k| {
                    if let RedisValue::SimpleString(key) = k {
                        ctx.open_key(&key)
                            .get_value::<RedisJSON>(&REDIS_JSON_TYPE)
                            .and_then(|doc| {
                                if let Some(data) = doc {
                                    if let Some(value_index) = &data.value_index {
                                        if schema.name == value_index.index_name {
                                            add_document(key, &value_index.index_name, data)?;
                                        }
                                    }
                                    Ok(())
                                } else {
                                    Err("Error on get value from key".into())
                                }
                            })
                    } else {
                        Err("Error on parsing reply from scan".into())
                    }
                });
                res.map(|_| cursor)
            }
            _ => Err("Error on parsing reply from scan".into()),
        },
        _ => Err("Error on parsing reply from scan".into()),
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
    let path = args.next().unwrap_or_else(|| "$".to_string());

    let map = schema_map::as_ref();

    map.get(&index_name)
        .ok_or_else(|| "ERR no such index".into())
        .map(|schema| &schema.index)
        .and_then(|index| {
            let result =
                index
                    .search(&query)?
                    .try_fold(Value::Object(Map::new()), |mut acc, key| {
                        let redis_key = ctx.open_key(&key);

                        redis_key
                            .get_value::<RedisJSON>(&REDIS_JSON_TYPE)
                            .and_then(|doc| {
                                doc.map_or(Ok(Vec::new()), |data| {
                                    data.get_values(&path)
                                        .map_err(|e| e.into()) // Convert Error to RedisError
                                        .map(|values| values.into_iter().cloned().collect())
                                })
                            })
                            .map(|r| {
                                acc.as_object_mut()
                                    .unwrap()
                                    .insert(key.to_string(), Value::Array(r));
                                acc
                            })
                    })?;

            Ok(RedisJSON::serialize(&result, Format::JSON)?.into())
        })
}
