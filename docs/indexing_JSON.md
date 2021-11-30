# Indexing JSON documents

In addition to (JSON) documents store, it is also possible to index JSON documents, gaining full-text search capabilities and document retrieving based on their content. To do so, you must install both modules, RedisJSON and RediSearch.

## Prerequisites

What do you need to start indexing JSON documents?

- Redis 6.x or later
- RediJSON 2.0 or later
- RediSearch 2.2 or later

## How to index JSON documents

Let's start by creating an index.

We can now specify `ON JSON` to inform RediSearch that we want to index JSON documents.

Then, on the `SCHEMA` part, you can provide JSONPath expressions.
The result of each `JSON Path` expression is indexed and associated with a logical name (`attribute`).
Use the attribute name in the query part.

Here is the basic syntax for indexing a JSON document:

    FT.CREATE {index_name} ON JSON SCHEMA {json_path} AS {attribute} {type}

And here's a concrete example:

    FT.CREATE userIdx ON JSON SCHEMA $.user.name AS name TEXT $.user.tag AS country TAG

Note: The attribute is optional in `FT.CREATE`, but it is required to use attributes in attribute modifiers in the query of `FT.SEARCH` and  `FT.AGGREGATE`, and avoid using JSON Path expressions, which are not fully supported by the query parser.

## Adding JSON document to the index

As soon as the index is created, any pre-existing JSON document or any new JSON document added or modified is automatically indexed.

We are free to use any writing command from the RedisJSON module (`JSON.SET`, `JSON.ARRAPPEND`, etc).

In this example we are going to use the following JSON document:

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

We can use `JSON.SET` to store in our database:

```sql
    JSON.SET myDoc $ '{"user":{"name":"John Smith","tag":"foo,bar","hp":1000, "dmg":150}}'
```

Because indexing is synchronous, the document will be visible on the index as soon as the `JSON.SET` command returns.
Any subsequent query matching the indexed content will return the document.

## Searching

To search for documents, we use the [FT.SEARCH](Commands.md#FT.SEARCH) commands.
We can search for any attribute mentioned in the schema.

Following our example, let's find our user called `John`:

```sql
FT.SEARCH userIdx '@name:(John)'
1) (integer) 1
2) "myDoc"
3) 1) "$"
   2) "{\"user\":{\"name\":\"John Smith\",\"tag\":\"foo,bar\",\"hp\":1000,\"dmg\":150}}"
```

## Field projection

We just saw that, by default, `FT.SEARCH` returns the whole document.

We can also return only specific attributes (here `name`):

```sql
FT.SEARCH userIdx '@name:(John)' RETURN 1 name
1) (integer) 1
2) "myDoc"
3) 1) "name"
   2) "\"John Smith\""
```

### Projecting using JSON Path expressions

The `RETURN` parameter also accepts a `JSON Path expression` letting us extract any part of the JSON document.

In this example, we return the result of the JSON Path expression `$.user.hp`.

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

We can highlight any attribute as soon as it is indexed using the TEXT type.

In FT.SEARCH we have to explicitly set the attributes in the `RETURN` parameter and the `HIGHLIGHT` parameters.

```sql
FT.SEARCH userIdx '@name:(John)' RETURN 1 name HIGHLIGHT FIELDS 1 name TAGS '<b>' '</b>'
1) (integer) 1
2) "myDoc"
3) 1) "name"
   2) "\"<b>John</b> Smith\""
```

## Aggregation with JSON Path expression

[Aggregation](Aggregations.md) is a powerful feature. You can use it to generate statistics or build facet queries.
The LOAD parameter accepts JSON Path expressions. Any value (even not indexed) can be used in the pipeline.

In this example, we are loading two numeric values from the JSON document applying a simple operation.

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

### JSON arrays can only be indexed in a TAG fields.

It is only possible to index an array of strings or booleans in a TAG field.
Other types (numeric, geo, null) are not supported.

### It is not possible to index JSON objects.

To be indexed, a JSONPath expression must return a single scalar value (string or number).
If the JSONPath expression returns an object or an array, it will be ignored.

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

If we want to index the string values part of the array under the `address` key, we must create two fields:

```sql
FT.CREATE orgIdx ON JSON SCHEMA $.address[0] AS a1 TEXT $.address[1] AS a2 TEXT
OK
```

We can now index the document:

```sql
JSON.SET org:1 $ '{"name": "Headquarters","address": ["Suite 250","Mountain View"],"cp": "CA 94040"}'
OK
```

We can now search in the address:

```sql
FT.SEARCH orgIdx "suite 250"
1) (integer) 1
2) "org:1"
3) 1) "$"
   2) "{\"name\":\"Headquarters\",\"address\":[\"Suite 250\",\"Mountain View\"],\"cp\":\"CA 94040\"}"
```

### JSON strings and numbers as to be indexed as TEXT and NUMERIC

- JSON Strings can only be indexed as TEXT, TAG, and GEO (using the correct syntax).
- JSON numbers can only be indexed as NUMERIC.
- JSON booleans can only be indexed as TAG.
- NULL values are ignored.

### SORTABLE is not supported on TAG

```sql
FT.CREATE orgIdx ON JSON SCHEMA $.cp[0] AS cp TAG SORTABLE
(error) On JSON, cannot set tag field to sortable - cp
```

With hashes, SORTABLE can be used (as a side effect) to improve the performance of FT.AGGREGATE on TAGs.
This is possible because the value in the hash is a string. Eg.: "foo,bar".

With JSON it is possible to index an array of string values.
Because there is no valid single textual representation of those values,
there is no way for RediSearch to know how to sort the result.