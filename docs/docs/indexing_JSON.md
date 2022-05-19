---
title: "Search/Indexing JSON documents"
linkTitle: "Search/Indexing"
weight: 2
description: >
    Searching and indexing JSON documents
---

In addition to storing JSON documents, you can also index them using the RediSearch module. This enables full-text search capabilities and document retrieval based on their content. To use this feature, you must install two modules: RedisJSON and RediSearch.

## Prerequisites

What do you need to start indexing JSON documents?

- Redis 6.x or later
- RedisJSON 2.0 or later
- RediSearch 2.2 or later

## How to index JSON documents

This section shows how to create an index.

You can now specify `ON JSON` to inform RediSearch that you want to index JSON documents.

For the `SCHEMA`, you can provide JSONPath expressions.
The result of each `JSON Path` expression is indexed and associated with a logical name (`attribute`).
Use the attribute name in the query.

Here is the basic syntax for indexing a JSON document:

    FT.CREATE {index_name} ON JSON SCHEMA {json_path} AS {attribute} {type}

And here's a concrete example:

    FT.CREATE userIdx ON JSON SCHEMA $.user.name AS name TEXT $.user.tag AS country TAG

Note: The attribute  is optional in `FT.CREATE`, but `FT.SEARCH` and  `FT.AGGREGATE` queries require attribute modifiers. You should also avoid using JSON Path expressions, which are not fully supported by the query parser.

## Adding a JSON document to the index

As soon as the index is created, any pre-existing JSON document or any new JSON document, added or modified, is automatically indexed.

You can use any write command from the RedisJSON module (`JSON.SET`, `JSON.ARRAPPEND`, etc.).

This example uses the following JSON document:

```JSON
{
  "user": {
    "name": "John Smith",
    "tag": "foo,bar",
    "hp": "1000",
    "dmg": "150"
  }
}
```

Use `JSON.SET` to store the document in the database:

```sql
    JSON.SET myDoc $ '{"user":{"name":"John Smith","tag":"foo,bar","hp":1000, "dmg":150}}'
```

Because indexing is synchronous, the document will be visible on the index as soon as the `JSON.SET` command returns.
Any subsequent query that matches the indexed content will return the document.

## Searching

To search for documents, use the `FT.SEARCH` command.
You can search any attribute mentioned in the schema.

Following our example, find the user called `John`:

```sql
FT.SEARCH userIdx '@name:(John)'
1) (integer) 1
2) "myDoc"
3) 1) "$"
   2) "{\"user\":{\"name\":\"John Smith\",\"tag\":\"foo,bar\",\"hp\":1000,\"dmg\":150}}"
```

## Indexing JSON arrays with tags

It is possible to index scalar string and boolean values in JSON arrays by using the wildcard operator in the JSON Path. For example if you were indexing blog posts you might have a field called `tags` which is an array of tags that apply to the blog post.

```JSON
{
   "title":"Using RedisJson is Easy and Fun",
   "tags":["redis","json","redisjson"]
}
```

You can apply an index to the `tags` field by specifying the JSON Path `$.tags.*` in your schema creation:

```bash
FT.CREATE blog-idx ON JSON PREFIX 1 Blog: SCHEMA $.tags.* AS tags TAG
```

You would then set a blog post as you would any other JSON document:

```bash
JSON.SET Blog:1 . '{"title":"Using RedisJson is Easy and Fun", "tags":["redis","json","redisjson"]}'
```

And finally you can search using the typical tag searching syntax:

```bash
127.0.0.1:6379> FT.SEARCH blog-idx "@tags:{redis}"
1) (integer) 1
2) "Blog:1"
3) 1) "$"
   2) "{\"title\":\"Using RedisJson is Easy and Fun\",\"tags\":[\"redis\",\"json\",\"redisjson\"]}"
```

## Field projection

`FT.SEARCH` returns the whole document by default.

You can also return only a specific attribute (`name` for example):

```sql
FT.SEARCH userIdx '@name:(John)' RETURN 1 name
1) (integer) 1
2) "myDoc"
3) 1) "name"
   2) "\"John Smith\""
```

### Projecting using JSON Path expressions

The `RETURN` parameter also accepts a `JSON Path expression` which lets you extract any part of the JSON document.

The following example returns the result of the JSON Path expression `$.user.hp`.

```sql
FT.SEARCH userIdx '@name:(John)' RETURN 1 $.user.hp
1) (integer) 1
2) "myDoc"
3) 1) "$.user.hp"
   2) "1000"
```

Note that the property name is the JSON expression itself: `3) 1) "$.user.hp"`

Using the `AS` option, it is also possible to alias the returned property.

```sql
FT.SEARCH userIdx '@name:(John)' RETURN 3 $.user.hp AS hitpoints
1) (integer) 1
2) "myDoc"
3) 1) "hitpoints"
   2) "1000"
```

### Highlighting

You can highlight any attribute as soon as it is indexed using the TEXT type.

For FT.SEARCH, you have to explicitly set the attributes in the `RETURN` parameter and the `HIGHLIGHT` parameters.

```sql
FT.SEARCH userIdx '@name:(John)' RETURN 1 name HIGHLIGHT FIELDS 1 name TAGS '<b>' '</b>'
1) (integer) 1
2) "myDoc"
3) 1) "name"
   2) "\"<b>John</b> Smith\""
```

## Aggregation with JSON Path expression

Aggregation is a powerful feature. You can use it to generate statistics or build facet queries.
The LOAD parameter accepts JSON Path expressions. Any value (even not indexed) can be used in the pipeline.

This example loads two numeric values from the JSON document applying a simple operation.

```sql
FT.AGGREGATE userIdx '*' LOAD 6 $.user.hp AS hp $.user.dmg AS dmg APPLY '@hp-@dmg' AS points
1) (integer) 1
2) 1) "hp"
   2) "1000"
   3) "dmg"
   4) "150"
   5) "points"
   6) "850"
```

## Current indexing limitations

### JSON arrays can only be indexed in TAG identifiers.

It is only possible to index an array of strings or booleans in a TAG identifier.
Other types (numeric, geo, null) are not supported.

### It is not possible to index JSON objects.

To be indexed, a JSONPath expression must return a single scalar value (string or number).

If the JSONPath expression returns an object, it will be ignored.

However it is possible to index the strings in separated attributes.

Given the following document:

```JSON
{
  "name": "Headquarters",
  "address": [
    "Suite 250",
    "Mountain View"
  ],
  "cp": "CA 94040"
}
```

Before you can index the array under the `address` key, you have to create two fields:

```sql
FT.CREATE orgIdx ON JSON SCHEMA $.address[0] AS a1 TEXT $.address[1] AS a2 TEXT
OK
```

You can now index the document:

```sql
JSON.SET org:1 $ '{"name": "Headquarters","address": ["Suite 250","Mountain View"],"cp": "CA 94040"}'
OK
```

You can now search in the address:

```sql
FT.SEARCH orgIdx "suite 250"
1) (integer) 1
2) "org:1"
3) 1) "$"
   2) "{\"name\":\"Headquarters\",\"address\":[\"Suite 250\",\"Mountain View\"],\"cp\":\"CA 94040\"}"
```

### Index JSON strings and numbers as TEXT and NUMERIC

- You can only index JSON strings as TEXT, TAG, or GEO (using the correct syntax).
- You can only index JSON numbers as NUMERIC.
- JSON booleans can only be indexed as TAG.
- NULL values are ignored.

### SORTABLE is not supported on TAG

```sql
FT.CREATE orgIdx ON JSON SCHEMA $.cp[0] AS cp TAG SORTABLE
(error) On JSON, cannot set tag field to sortable - cp
```

With hashes, you can use SORTABLE (as a side effect) to improve the performance of FT.AGGREGATE on TAGs.
This is possible because the value in the hash is a string, such as "foo,bar".

With JSON, you can index an array of strings.
Because there is no valid single textual representation of those values,
there is no way for RediSearch to know how to sort the result.
