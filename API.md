Redis Modules API reference manual
===

Redis modules make possible the extension of Redis functionality using 
external modules, by creating new Redis commands with performance and 
features similar to what can be done inside the core itself.

Redis modules are dynamic libraries loaded into Redis at startup or 
using the `MODULE LOAD` command. Redis exports a C API, in the
form of a single C header file called `redismodule.h`. Modules are meant
to be written in C, or any language with C binding functionality 
like C++.

Modules are Redis-version agnostic: a given module does not need to be 
designed, or recompiled, in order to run with a specific version of 
Redis. In addition, they are registered using a specific Redis modules
API version. The current API version is "1".

This document describes the  alpha version of Redis modules. API, 
functionality and other details may change in the future.

# Loading modules

In order to test a new Redis module, use the following `redis.conf` 
configuration directive:

    loadmodule /path/to/mymodule.so

Load a module at runtime with the following command:

    MODULE LOAD /path/to/mymodule.so

To list all loaded modules, use:

    MODULE LIST

Finally, unload (or reload) a module using the following command:

    MODULE UNLOAD mymodule

Note that `mymodule` is the name the module used to register itself 
with the Redis core, and **is not** the filename without the 
`.so` suffix. The name can be obtained using `MODULE LIST`. It is 
recommended to use the same filename for the dynamic library and module.

# A Hello World module

In order illustrate the basic components of a module, the following 
implements a command that outputs a random number.

    #include "redismodule.h"
    #include <stdlib.h>

    int HelloworldRand_RedisCommand(RedisModuleCtx *ctx, RedisModuleString **argv, int argc) {
        RedisModule_ReplyWithLongLong(ctx,rand());
        return REDISMODULE_OK;
    }

    int RedisModule_OnLoad(RedisModuleCtx *ctx) {
        if (RedisModule_Init(ctx,"helloworld",1,REDISMODULE_APIVER_1)
            == REDISMODULE_ERR) return REDISMODULE_ERR;

        if (RedisModule_CreateCommand(ctx,"helloworld.rand",
            HelloworldRand_RedisCommand) == REDISMODULE_ERR)
            return REDISMODULE_ERR;

        return REDISMODULE_OK;
    }

The example module has two functions. One implements a command called
`HELLOWORLD.RAND`. This function is specific to that module. In 
addition, `RedisModule_OnLoad()` must be present in each Redis module. 
It is the entry point for module initialization, and registers its 
commands and other private data structures.

In order to avoid namespace collisions, module commands should use the 
dot-notation, for example, `HELLOWORLD.RAND`.

Namespace collisions will cause `RedisModule_CreateCommand` to fail in 
one or more modules. Loading will abort and an error condition
returned.

# Module initialization

The above example shows the usage of the function `RedisModule_Init()`.
It should be the first function called by the module `OnLoad` function.
The following is the function prototype:

    int RedisModule_Init(RedisModuleCtx *ctx, const char *modulename,
                         int module_version, int api_version);

The `Init` function announces to the Redis core that the module has a 
given name, its version (reported by `MODULE LIST`), and that it uses 
a specific version of the API.

If the API version is wrong, the name is already taken, or there are other
similar errors, the function will return `REDISMODULE_ERR`, and the module
`OnLoad` function should return ASAP with an error.

Before the `Init` function is called, no other API function can be called,
otherwise the module will segfault and the Redis instance will crash.

The second function called, `RedisModule_CreateCommand`, registers 
commands with the Redis core. The following is the prototype:

    int RedisModule_CreateCommand(RedisModuleCtx *ctx, const char *cmdname,
                                  RedisModuleCmdFunc cmdfunc);

Most Redis modules API calls have the `context` of the module 
as first argument, in order to reference the calling module's context
and the client executing a given command.

To create a new command, the above function needs the context, the command
name, and the function pointer of the function implementing the command,
which must have the following prototype:


    int mycommand(RedisModuleCtx *ctx, RedisModuleString **argv, int argc);

The command function arguments are just the context, that will be passed
to all the other API calls, the command argument vector, and total number
of arguments, as passed by the user.

The arguments are provided as pointers to a specific data type, the 
`RedisModuleString`. This is an opaque data type with API functions 
enabling access and use. Direct access to its fields is never needed.

Zooming into the example command implementation, we can find another call:

    int RedisModule_ReplyWithLongLong(RedisModuleCtx *ctx, long long integer);

This function returns an integer to the client that invoked the command,
exactly like other Redis commands do, like for example `INCR` or `SCARD`.

# Setup and dependencies of a Redis module

Redis modules do not depend on Redis or 3rd party libraries, nor do they
need to be compiled with a specific `redismodule.h` file. In order
to create a new module, just copy a recent version of `redismodule.h`
in your source tree, link all the libraries you want, and create
a dynamic library and export the `RedisModule_OnLoad()` function symbol.

The module will be able to load into different versions of Redis.

# Working with RedisModuleString objects

The command argument vector `argv` passed to module commands, and the
return value of other module APIs functions, are of type `RedisModuleString`.

Most often Redis module strings are passed directly to other API calls.
However, a number of API functions enable direct access to the string 
object. For example, 

    const char *RedisModule_StringPtrLen(RedisModuleString *string, size_t *len);

returns a pointer to `string` and sets its length in `len`. The `const` 
modifier prevents direct modification.

New string objects are created using the following API:

    RedisModuleString *RedisModule_CreateString(RedisModuleCtx *ctx, const char *ptr, size_t len);

The string returned by the above command must be freed using a corresponding
call to `RedisModule_FreeString()`:

    void RedisModule_FreeString(RedisModuleString *str);

Alternatively, the automatic memory management API, covered later in 
this document, can be employed to automatically free string handles.

Note that the strings provided via the argument vector `argv` never need
to be freed. Only strings created within a module need to be freed, or 
new strings returned by other APIs where specified.

## Creating strings from numbers or parsing strings as numbers

Creating a new string from an integer is a very common operation, so there
is a function to do this:

    RedisModuleString *mystr = RedisModule_CreateStringFromLongLong(ctx,10);

Similarly in order to parse a string as a number:

    long long myval;
    if (RedisModule_StringToLongLong(ctx,argv[1],&myval) == REDISMODULE_OK) {
        /* Do something with 'myval' */
    }

## Accessing Redis keys from modules

Most Redis modules, in order to be useful, have to interact with the Redis
data space. An exception would be an ID generator that never accesses 
Redis keys. Redis modules have two different APIs in order to
access the Redis data space. A **low level API** providing very
fast access and a set of functions to manipulate Redis data structures.
A **high level API** is provided to allow calling Redis commands and
retrieving results, similar to how Lua scripts access Redis.

The high level API is also useful in order to access Redis functionalities
that are not available as APIs.

In general, module developers should prefer the low level API, because commands
implemented using the low level API run at a speed comparable to the speed
of native Redis commands. However there are definitely use cases for the
higher level API. For example often the bottleneck could be processing the
data and not accessing it.

Also note that in some cases the low level API is is as simple as the 
high level API.

# Calling Redis commands

The high level API to access Redis combines the `RedisModule_Call()`
function with the functions needed to access the reply object returned 
by `Call()`.

`RedisModule_Call` uses a special calling convention, with a format 
specifier used to define the types of objects passed as arguments.

Redis commands are invoked using a command name and a list of arguments.
However, when calling commands, the arguments may originate from different
kind of strings: null-terminated C strings, RedisModuleString objects as
received from the `argv` parameter in the command implementation, binary
safe C buffers with a pointer and a length, and so forth.

To call `INCRBY` using as first argument (the key) a string received in 
the argument vector `argv`, which is an array of RedisModuleString 
object pointers, and a C string representing the number "10" as a second 
argument (the increment), use the following function call:

    RedisModuleCallReply *reply;
    reply = RedisModule_Call(ctx,"INCR","sc",argv[1],"10");

The first argument is the context, and the second is always a null terminated
C string with the command name. The third argument is the format specifier
where each character corresponds to the type of the arguments that will follow.
In the above case `"sc"` means a RedisModuleString object, and a null
terminated C string. The other arguments are just the two arguments as
specified. In fact `argv[1]` is a RedisModuleString and `"10"` is a null
terminated C string.

This is the full list of format specifiers:

* **c** -- Null terminated C string pointer.
* **b** -- C buffer, two arguments needed: C string pointer and `size_t` length.
* **s** -- RedisModuleString as received in `argv` or by other Redis module APIs returning a RedisModuleString object.
* **l** -- Long long integer.
* **v** -- Array of RedisModuleString objects.
* **!** -- This modifier just tells the function to replicate the command to slaves and AOF. It is ignored from the point of view of arguments parsing.

The function returns either a `RedisModuleCallReply` object on success, 
or NULL on error.

NULL is returned when the command name is invalid, the format specifier 
uses characters that are not recognized, or when the command is called 
with the wrong number of arguments. In the above cases the `errno` var 
is set to `EINVAL`. NULL is also returned when, in an instance with 
Cluster enabled, the target keys are non local hash slots. In this
case `errno` is set to `EPERM`.

## Working with RedisModuleCallReply objects.

`RedisModuleCall` returns reply objects that can be accessed using the
`RedisModule_CallReply*` family of functions.

In order to obtain the type or reply (corresponding to one of the data types
supported by the Redis protocol), the function `RedisModule_CallReplyType()`
is used:

    reply = RedisModule_Call(ctx,"INCR","sc",argv[1],"10");
    if (RedisModule_CallReplyType(reply) == REDISMODULE_REPLY_INTEGER) {
        long long myval = RedisModule_CallReplyInteger(reply);
        /* Do something with myval. */
    }

Valid reply types are:

* `REDISMODULE_REPLY_STRING` Bulk string or status replies.
* `REDISMODULE_REPLY_ERROR` Errors.
* `REDISMODULE_REPLY_INTEGER` Signed 64 bit integers.
* `REDISMODULE_REPLY_ARRAY` Array of replies.
* `REDISMODULE_REPLY_NULL` NULL reply.

Strings, errors and arrays have an associated length. For strings and errors
the length corresponds to the length of the string. For arrays the length
is the number of elements. To obtain the reply length the following function
is used:

    size_t reply_len = RedisModule_CallReplyLength(reply);

In order to obtain the value of an integer reply, the following function
is used, as already shown in the example above:

    long long reply_integer_val = RedisModule_CallReplyInteger(reply);

Called with a reply object of the wrong type, the above function always
returns `LLONG_MIN`.

Sub elements of array replies are accessed this way:

    RedisModuleCallReply *subreply;
    subreply = RedisModule_CallReplyArrayElement(reply,idx);

The above function returns NULL if you try to access out of range elements.

Strings and errors (which are like strings but with a different type) can
be accessed in the following way, making sure to never write to
the resulting pointer (that is returned as as const pointer so that 
misusing must be pretty explicit)::

    size_t len;
    const char *ptr = RedisModule_CallReplyStringPtr(reply,&len);

If the reply type is not a string or an error, NULL is returned.

RedisCallReply objects are not the same as module string objects
(RedisModuleString types). When an API function expects a module string, 
the following function can be employed to create a new 
`RedisModuleString` object from a call reply of type string, error or 
integer:

    RedisModuleString *mystr = RedisModule_CreateStringFromCallReply(myreply);

Alternatively, one could evaluate whether using the low level API is as 
simple (and potentially faster).

If the reply is not of the right type, NULL is returned. The returned 
string object should be released with `RedisModule_FreeString()`, or by 
enabling automatic memory management (see section below).

# Releasing call reply objects

Reply objects must be freed using `RedisModule_FreeCallRelpy`. For arrays,
only the top level reply needs to be freed, but not the nested replies.
Currently, the module implementation provides protection in order to avoid
crashing if a nested reply object is freed on error--however, *this 
protective feature may not be available in future versions, and should 
not be considered part of the API*.

Automatic memory management can take care of freeing replies (see 
section below). Alternatively, memory can be released ASAP.

## Returning values from Redis commands

Like normal Redis commands, new commands implemented via modules must be
able to return values to the caller. Towards this end, The API exports a
set of functions in order to return the usual Redis protocol types, and
arrays of such types (as elemented). Also, errors can be returned with any
error code and string, where the error code is the initial uppercase 
letters in the error message--for example, the "BUSY" string in the 
"BUSY the sever is busy" error message.

All the functions to send a reply to the client are called
`RedisModule_ReplyWith<something>`.

To return an error, use:

    RedisModule_ReplyWithError(RedisModuleCtx *ctx, const char *err);

There is a predefined error string for key of wrong type errors:

    REDISMODULE_ERRORMSG_WRONGTYPE

Example usage:

    RedisModule_ReplyWithError(ctx,"ERR invalid arguments");

We already saw how to reply with a long long in the examples above:

    RedisModule_ReplyWithLongLong(ctx,12345);

To reply with a simple string like "OK", that can't contain binary 
values or newlines, use:

    RedisModule_ReplyWithSimpleString(ctx,"OK");

It's possible to reply with "bulk strings" that are binary safe, using
two different functions:

    int RedisModule_ReplyWithStringBuffer(RedisModuleCtx *ctx, const char *buf, size_t len);

    int RedisModule_ReplyWithString(RedisModuleCtx *ctx, RedisModuleString *str);

The first function gets a C pointer and length. The second a RedisModuleString
object. Use one or the other depending on the source type you have at hand.

In order to reply with an array, use a function to emit the array 
length, followed by as many calls to the above functions as there are
elements in the array:

    RedisModule_ReplyWithArray(ctx,2);
    RedisModule_ReplyWithStringBuffer(ctx,"age",3);
    RedisModule_ReplyWithLongLong(ctx,22);

Returning nested arrays is easy--the nested array element just uses another
call to `RedisModule_ReplyWithArray()` followed by the calls to emit the
sub array elements.

## Returning arrays with dynamic length

Sometimes it is not possible to know beforehand the number of items of
an array. As an example, think of a Redis module implementing a FACTOR
command that given a number outputs the prime factors. Instead of
factorializing the number, storing the prime factors in an array, and
then producing the command reply, a better solution is to start an array
reply where the length is not known, and set it later. This is accomplished
with a special argument to `RedisModule_ReplyWithArray()`:

    RedisModule_ReplyWithArray(ctx, REDISMODULE_POSTPONED_ARRAY_LEN);

The above call starts an array reply, and more `ReplyWith` calls can 
then be used to produce the array items. Finally in order to set the 
length use the following call:

    RedisModule_ReplySetArrayLength(ctx, number_of_items);

In the case of the FACTOR command, this translates to some code similar
to this:

    RedisModule_ReplyWithArray(ctx, REDISMODULE_POSTPONED_ARRAY_LEN);
    number_of_factors = 0;
    while(still_factors) {
        RedisModule_ReplyWithLongLong(ctx, some_factor);
        number_of_factors++;
    }
    RedisModule_ReplySetArrayLength(ctx, number_of_factors);

Another common use case for this feature is iterating over the arrays of
some collection and only returning the ones passing some kind of filtering.

It is possible to have multiple nested arrays with a postponed reply.
Each call to `SetArray()` will set the length of the latest corresponding
call to `ReplyWithArray()`:

    RedisModule_ReplyWithArray(ctx, REDISMODULE_POSTPONED_ARRAY_LEN);
    ... generate 100 elements ...
    RedisModule_ReplyWithArray(ctx, REDISMODULE_POSTPONED_ARRAY_LEN);
    ... generate 10 elements ...
    RedisModule_ReplySetArrayLength(ctx, 10);
    RedisModule_ReplySetArrayLength(ctx, 100);

This creates a 100 items array having as last element a 10 items array.

# Arity and type checks

Often commands need to check that the number of arguments and type of the key
is correct. To report a wrong arity, use `RedisModule_WrongArity()`:

    if (argc != 2) return RedisModule_WrongArity(ctx);

Checking for the wrong type involves opening the key and checking the type:

    RedisModuleKey *key = RedisModule_OpenKey(ctx,argv[1],
        REDISMODULE_READ|REDISMODULE_WRITE);

    int keytype = RedisModule_KeyType(key);
    if (keytype != REDISMODULE_KEYTYPE_STRING &&
        keytype != REDISMODULE_KEYTYPE_EMPTY)
    {
        RedisModule_CloseKey(key);
        return RedisModule_ReplyWithError(ctx,REDISMODULE_ERRORMSG_WRONGTYPE);
    }

Note that you often want to proceed with a command both if the key
is of the expected type, or if it's empty.

## Low level access to keys

Low level access to keys enable operations on value objects associated
with keys directly, with a speed similar to what Redis uses internally to
implement the built-in commands.

Once a key is opened, a key pointer is returned that will be used with all the
other low level API calls in order to perform operations on the key or its
associated value.

Because the API is meant to be very fast, it cannot do too many run-time
checks, so the user must be aware of certain rules to follow:

* Opening the same key multiple times where at least one instance is 
opened for writing, is undefined and may lead to crashes.
* While a key is open, it should only be accessed via the low level key 
API. For example opening a key, then calling DEL on the same key using 
the `RedisModule_Call()` API will result in a crash. However it is safe 
to open a key, perform some operation with the low level API, close it, 
then use other APIs to manage the same key, and later opening it again 
to do some more work.

In order to open a key the `RedisModule_OpenKey` function is used. It 
returns a key pointer, used in subsequent calls to access and modify
the value:

    RedisModuleKey *key;
    key = RedisModule_OpenKey(ctx,argv[1],REDISMODULE_READ);

The second argument is the key name, and must be a `RedisModuleString` 
object. The third argument is the mode: `REDISMODULE_READ` or 
`REDISMODULE_WRITE`. It is possible to use `|` to bitwise OR the two 
modes to open the key in both modes. Currently, a key opened for writing
can also be accessed for reading but this is to be considered an 
implementation detail. The right mode should be used in sane modules.

You can open non-existent keys for writing, and the keys will be created
when an attempt to write to the key is performed. However when opening 
keys just for reading, `RedisModule_OpenKey` will return NULL if the key
does not exist.

A key is closed by calling:

    RedisModule_CloseKey(key);

With automatic memory management enabled, Redis will close all open keys 
when the module function returns

## Getting the key type

In order to obtain the value of a key, use `RedisModule_KeyType()`:

    int keytype = RedisModule_KeyType(key);

It returns one of the following values:

    REDISMODULE_KEYTYPE_EMPTY
    REDISMODULE_KEYTYPE_STRING
    REDISMODULE_KEYTYPE_LIST
    REDISMODULE_KEYTYPE_HASH
    REDISMODULE_KEYTYPE_SET
    REDISMODULE_KEYTYPE_ZSET

The above are standard Redis key types, with the addition of an empty
type signalling the key pointer is associated with an empty key that
does not exist yet.

## Creating new keys

To create a new key, open it for writing and then write to it using one
of the key writing functions. Example:

    RedisModuleKey *key;
    key = RedisModule_OpenKey(ctx,argv[1],REDISMODULE_READ);
    if (RedisModule_KeyType(key) == REDISMODULE_KEYTYPE_EMPTY) {
        RedisModule_StringSet(key,argv[2]);
    }

## Deleting keys

Just use:

    RedisModule_DeleteKey(key);

The function returns `REDISMODULE_ERR` if the key is not open for writing.
Note that after a key gets deleted, it is setup in order to be targeted
by new key commands. For example `RedisModule_KeyType()` will return it 
as an empty key, and writing to it will create a new key, possibly of 
another type (depending on the API used).

## Managing key expires (TTLs)

To control key expires two functions are provided, that are able to set,
modify, get, and unset the time to live associated with a key.

One function is used in order to query the current expire of an open key:

    mstime_t RedisModule_GetExpire(RedisModuleKey *key);

The function returns the time to live of the key in milliseconds, or
`REDISMODULE_NO_EXPIRE` as a special value to signal the key has no associated
expire or does not exist at all (you can differentiate the two cases checking
if the key type is `REDISMODULE_KEYTYPE_EMPTY`).

In order to change the expire of a key the following function is used instead:

    int RedisModule_SetExpire(RedisModuleKey *key, mstime_t expire);

When called on a non existing key, `REDISMODULE_ERR` is returned, because
the function can only associate expires to existing open keys (non existing
open keys are only useful in order to create new values with data type
specific write operations).

Again the `expire` time is specified in milliseconds. If the key has currently
no expire, a new expire is set. If the key already have an expire, it is
replaced with the new value.

If the key has an expire, and the special value `REDISMODULE_NO_EXPIRE` is
used as a new expire, the expire is removed, similarly to the Redis
`PERSIST` command. In case the key was already persistent, no operation is
performed.

## Obtaining the length of values

There is a single function in order to retrieve the length of the value
associated with an open key. The returned length is value-specific, and is
the string length for strings, and the number of elements for the aggregated
data types (how many elements there is in a list, set, sorted set, hash).

    size_t len = RedisModule_ValueLength(key);

If the key does not exist, 0 is returned by the function:

## String type API

Setting a new string value, like the Redis `SET` command does, is performed
using:

    int RedisModule_StringSet(RedisModuleKey *key, RedisModuleString *str);

The function works exactly like the Redis `SET` command itself, that is, if
there is a prior value (of any type) it will be deleted.

Accessing existing string values is performed using DMA (direct memory
access) for speed. The API will return a pointer and a length, so that
it's possible to access and, if needed, modify the string directly.

    size_t len, j;
    char *myptr = RedisModule_StringDMA(key,REDISMODULE_WRITE,&len);
    for (j = 0; j < len; j++) myptr[j] = 'A';

In the above example we write directly on the string. Note that if you want
to write, you must be sure to ask for `WRITE` mode.

DMA pointers are only valid if no other operations are performed with the key
before using the pointer, after the DMA call.

Sometimes when we want to manipulate strings directly, we need to change
their size as well. For this scope, the `RedisModule_StringTruncate` function
is used. Example:

    RedisModule_StringTruncate(mykey,1024);

The function truncates, or enlarges the string as needed, padding it with
zero bytes if the previos length is smaller than the new length we request.
If the string does not exist since `key` is associated to an open empty key,
a string value is created and associated to the key.

Note that every time `StringTruncate()` is called, we need to re-obtain
the DMA pointer again, since the old may be invalid.

## List type API

It's possible to push and pop values from list values:

    int RedisModule_ListPush(RedisModuleKey *key, int where, RedisModuleString *ele);
    RedisModuleString *RedisModule_ListPop(RedisModuleKey *key, int where);

The `where` argument specifies whether to push or pop from the tail
or head, using the following macros:

    REDISMODULE_LIST_HEAD
    REDISMODULE_LIST_TAIL

Elements returned by `RedisModule_ListPop()` are like strings created with
`RedisModule_CreateString()`, they must be released with
`RedisModule_FreeString()` or by enabling automatic memory management.

## Set type API

See [FUNCTIONS.md](FUNCTIONS.md)

## Sorted set type API

See [FUNCTIONS.md](FUNCTIONS.md)

## Hash type API

See [FUNCTIONS.md](FUNCTIONS.md)

## Iterating aggregated values

See [FUNCTIONS.md](FUNCTIONS.md)

# Replicating commands

If you want to use module commands exactly like normal Redis commands, in the
context of replicated Redis instances, or using the AOF file for persistence,
it is important for module commands to handle their replication in a consistent
way.

When using the higher level APIs to invoke commands, replication happens
automatically when using the "!" modifier in the format string of
`RedisModule_Call()` as in the following example:

    reply = RedisModule_Call(ctx,"INCR","!sc",argv[1],"10");

The bang is not parsed as a format specifier, but it internally flags 
the command as "must replicate".

For more complex scenarios than that, use the low level API. In this 
case, if there are no side effects in the command execution, and
it always consistently performs the same work, it is possible 
to replicate the command verbatim as the user executed it. To do that, 
call the following function:

    RedisModule_ReplicateVerbatim(ctx);

When you using the above API, do not use any other replication function
since they are not guaranteed to mix well.

An alternative is to tell Redis exactly which commands to replicate as 
the effect of the command execution, using an API similar to 
`RedisModule_Call()`. Instead of calling the command they are sent to 
the AOF / slaves stream. For example:

    RedisModule_Replicate(ctx,"INCRBY","cl","foo",my_increment);

It's possible to call `RedisModule_Replicate` multiple times, and each
will emit a command. The entire sequence emitted is wrapped in a
`MULTI/EXEC` transaction, so that the AOF and replication effects are the
same as executing a single command.

It is not a good idea to mix both forms of replication if there are 
simpler alternatives. However, when mixing note that commands replicated
with `Call()` are always the first emitted in the final `MULTI/EXEC` 
block, while all the commands emitted with `Replicate()` will follow.

# Automatic memory management

Normally when writing programs in the C language, programmers need to manage
memory manually. This is why the Redis modules API has functions to release
strings, close open keys, free replies, and so forth.

However since commands are executed in a contained environment and
with a set of strict APIs, Redis is able to provide automatic memory 
management to modules, at the cost of some performance (most of the 
time, a very low cost).

When automatic memory management is enabled, there is **no need to**:

1. Close open keys.
2. Free replies.
3. Free RedisModuleString objects.

Automatic and manual memory management can be combined. For example, 
automatic memory management may be active, but inside a loop allocating 
a lot of strings, you may still want to free strings no longer used.

In order to enable automatic memory management, just call the following
function at the start of the command implementation:

    RedisModule_AutoMemory(ctx);

Automatic memory management is usually the way to go, however experienced
C programmers may not use it in order to gain some speed and memory usage
benefit.

# Writing commands compatible with Redis Cluster

Work in progress, see [FUNCTIONS.md](FUNCTIONS.md) for the following API:

    RedisModule_IsKeysPositionRequest(ctx);
    RedisModule_KeyAtPos(ctx,pos);

Command implementations, on keys position request, must reply with
`REDISMODULE_KEYPOS_OK` to signal the request was processed, otherwise
Cluster returns an error for those module commands that are not able to
describe the position of keys.

