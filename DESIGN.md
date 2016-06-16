# ReJSON Module Design

## Abstract

The purpose of this module is to provide native support for JSON documents stored in redis, allowing users to:

1. Store a JSON blob.
2. Manipulate just a part of the json object without retrieving it to the client.
3. Retrieve just a portion of the object as JSON.
4. Store JSON SChema objects and validate JSON objects based on schema keys.
4. Index objects in secondary indexes based on their properties.

Later on we can use the inernal object implementation in this module to produce similar modules for other serialization formats,
namely XML and BSON.

## Design Considerations

* Documents are added as JSON but are stored in an internal representation and not as strings.
* Internal representation does not depend on any JSON parser or library, to allow connecting other formats to it later.
* The internal representation will initially be limited to the types supported by JSON, but can later be extended to types like timestamps, etc.
* Queries that include internal paths of objects will be expressed in JSON path expressionse (e.g. `foo.bar[3].baz`)
* We will not implement our own JSON parser and composer, but use existing libraries.
* The code apart from the implementation of the redis commands will not depend on redis and will be testable without being compiled as a module.

## External API

Proposed API for the module:

```
JSON.SET <key> <path> <json> [SCHEMA <schema_key>]

JSON.GET <key> [<path>]

JSON.MGET <path> <key> [<key> ...]

JSON.DEL <key> <path>

JSON.SETSCHEMA <key> <json> 
  Notes:
    1. not sure if needed, we can add a modifier on the generic SET
    2. indexing will be specified in the schema

JSON.VALIDATE <schema_key> <json>

Optional commands:

JSON.INCREMENTBY <key> <path> <num>
    (Might be an overkill ;)

JSON.EXPIRE <key> <path> <ttl>    

```

## Object Data Type

The internal representation of JSON objects will be stored in a redis data type called Object [TBD].

These will be optimized for memory efficiency and path search speed. 

See [src/object.h](src/object.h) for the API specification.

## QueryPath 

When updating, reading and deleting parts of json objects, we'll use path specifiers. 

These too will have internal representation disconnected from their JSON path representation. 

## Secondary Indexes

## Connecting a JSON parser / writer

## Conneting Other Parsers 