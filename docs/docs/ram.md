---
title: "Redis JSON RAM Usage"
linkTitle: "Memory Usage"
weight: 6
description: >
    Debugging memory consumption
---

Every key in Redis takes memory and requires at least the amount of RAM to store the key name, as
well as some per-key overhead that Redis uses. On top of that, the value in the key also requires
RAM.

Redis JSON stores JSON values as binary data after deserializing them. This representation is often more
expensive, size-wize, than the serialized form. The JSON data type uses at least 24 bytes (on
64-bit architectures) for every value, as can be seen by sampling an empty string with the
`JSON.DEBUG MEMORY` command:

```
127.0.0.1:6379> JSON.SET emptystring . '""'
OK
127.0.0.1:6379> JSON.DEBUG MEMORY emptystring
(integer) 24
```

This RAM requirement is the same for all scalar values, but strings require additional space
depending on their actual length. For example, a 3-character string will use 3 additional bytes:

```
127.0.0.1:6379> JSON.SET foo . '"bar"'
OK
127.0.0.1:6379> JSON.DEBUG MEMORY foo
(integer) 27
```

Empty containers take up 32 bytes to set up:

```
127.0.0.1:6379> JSON.SET arr . '[]'
OK
127.0.0.1:6379> JSON.DEBUG MEMORY arr
(integer) 32
127.0.0.1:6379> JSON.SET obj . '{}'
OK
127.0.0.1:6379> JSON.DEBUG MEMORY obj
(integer) 32
```

The actual size of a container is the sum of sizes of all items in it on top of its own
overhead. To avoid expensive memory reallocations, containers' capacity is scaled by multiples of 2
until a treshold size is reached, from which they grow by fixed chunks.

A container with a single scalar is made up of 32 and 24 bytes, respectively:
```
127.0.0.1:6379> JSON.SET arr . '[""]'
OK
127.0.0.1:6379> JSON.DEBUG MEMORY arr
(integer) 56
```

A container with two scalars requires 40 bytes for the container (each pointer to an entry in the
container is 8 bytes), and 2 * 24 bytes for the values themselves:
```
127.0.0.1:6379> JSON.SET arr . '["", ""]'
OK
127.0.0.1:6379> JSON.DEBUG MEMORY arr
(integer) 88
```

A 3-item (each 24 bytes) container will be allocated with capacity for 4 items, i.e. 56 bytes:

```
127.0.0.1:6379> JSON.SET arr . '["", "", ""]'
OK
127.0.0.1:6379> JSON.DEBUG MEMORY arr
(integer) 128
```

The next item will not require an allocation in the container, so usage will increase only by that
scalar's requirement, but another value will scale the container again:

```
127.0.0.1:6379> JSON.SET arr . '["", "", "", ""]'
OK
127.0.0.1:6379> JSON.DEBUG MEMORY arr
(integer) 152
127.0.0.1:6379> JSON.SET arr . '["", "", "", "", ""]'
OK
127.0.0.1:6379> JSON.DEBUG MEMORY arr
(integer) 208
```

This table gives the size (in bytes) of a few of the test files on disk and when stored using
JSON. The _MessagePack_ column is for reference purposes and reflects the length of the value
when stored using MessagePack.

| File                                   | Filesize  | Redis JSON | MessagePack |
| -------------------------------------- | --------- | ------ | ----------- |
| /tests/files/pass-100.json              | 380       | 1079   | 140         |
| /tests/files/pass-jsonsl-1.json         | 1441      | 3666   | 753         |
| /tests/files/pass-json-parser-0000.json | 3468      | 7209   | 2393        |
| /tests/files/pass-jsonsl-yahoo2.json    | 18446     | 37469  | 16869       |
| /tests/files/pass-jsonsl-yelp.json      | 39491     | 75341  | 35469       |

> Note: In the current version, deleting values from containers **does not** free the container's
allocated memory.
