---
title: "Path"
linkTitle: "Path"
weight: 3
description: >
    RedisJSON JSONPath
---

Since no standard for path syntax exists, RedisJSON implements its own. RedisJSON's syntax is based on common best practices and intentionally resembles [JSONPath](http://goessner.net/articles/JsonPath/).

RedisJSON currently supports two query syntaxes: JSONPath syntax and a legacy path syntax from the first version of RedisJSON.

RedisJSON decides which syntax to use depending on the first character of the path query. If the query starts with the character `$`, it uses JSONPath syntax. Otherwise, it defaults to legacy path syntax.

## JSONPath support (RedisJSON v2)

RedisJSON 2.0 introduces [JSONPath](http://goessner.net/articles/JsonPath/) support. It follows the syntax described by Goessner in his article.

A JSONPath query can resolve to several locations in the JSON documents. In this case, the JSON commands apply the operation to every possible location. This is a major improvement over the legacy query, which only operates on the first path.

Notice that the structure of the command response often differs when using JSONPath. See the [Commands](/commands) page for more details.

The new syntax supports bracket notation, which allows the use of special characters like colon ":" or whitespace in key names.

## Legacy Path syntax (RedisJSON v1)

The first version of RedisJSON had the following implementation. It is still supported in RedisJSON v2.

Paths always begin at the root of a RedisJSON value. The root is denoted by a period character (`.`). For paths that reference the root's children, it is optional to prefix the path with the root.

RedisJSON supports both dot notation and bracket notation for object key access. The following paths all refer to _bar_, which is a child of _foo_ under the root:

*   `.foo.bar`
*   `foo["bar"]`
*   `['foo']["bar"]`

To access an array element, enclose its index within a pair of square brackets. The index is 0-based, with 0 being the first element of the array, 1 being the next element, and so on. You can use negative offsets to access elements starting from the end of the array. For example, -1 is the last element in the array, -2 is the second to last element, and so on.

## JSON key names and path compatibility

By definition, a JSON key can be any valid JSON string. Paths, on the other hand, are traditionally based on JavaScript's (and Java's) variable naming conventions. Therefore, while it is possible to have RedisJSON store objects containing arbitrary key names, you can only access these keys via a path if they conform to these naming syntax rules:

1.  Names must begin with a letter, a dollar sign (`$`), or an underscore (`_`) character
2.  Names can contain letters, digits, dollar signs, and underscores
3.  Names are case-sensitive

## Time complexity of path evaluation

The time complexity of searching (navigating to) an element in the path is calculated from:

1. Child level - every level along the path adds an additional search
2. Key search - O(N)<sup>&#8224;</sup>, where N is the number of keys in the parent object
3. Array search - O(1)

This means that the overall time complexity of searching a path is _O(N*M)_, where N is the depth and M is the number of parent object keys.

<sup>&#8224;</sup> while this is acceptable for objects where N is small, access can be optimized for larger objects. This optimization is planned for a future version.
