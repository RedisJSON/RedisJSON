# JSON with Redis server-side Lua

This is an implementation of ReJSON's `JSON.SET` and `JSON.GET` commands in **pure Redis Lua**. Yep,
that's right, no module needed.

It consists of two variants: the first uses JSON format to store data, whereas the other uses
MessagePack.
