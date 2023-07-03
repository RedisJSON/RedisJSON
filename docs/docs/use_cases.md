---
title: "Use cases"
linkTitle: "Use cases"
weight: 4
description: >
    JSON use cases
aliases:
  - /docs/stack/search/reference/query_syntax/    
---

You can of course use Redis native data structures to store JSON objects, and that's a common practice. For example, you can serialize JSON and save it in a Redis String.

However, Redis JSON provides several benefits over this approach.

**Access and retrieval of subvalues**

With JSON, you can get nested values without having to transmit the entire object over the network. Being able to access sub-objects can lead to greater efficiencies when you're storing large JSON objects in Redis.

**Atomic partial updates**

JSON allows you to atomically run operations like incrementing a value, adding, or removing elements from an array, append strings, and so on. To do the same with a serialized object, you have to retrieve and then reserialize the entire object, which can be expensive and also lack atomicity.

**Indexing and querying**

When you store JSON objects as Redis strings, there's no good way to query those objects. On the other hand, storing these objects as JSON using Redis Stack lets you index and query them. This is provided by the search and query capabilities of Redis Stack.