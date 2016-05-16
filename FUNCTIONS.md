# Generated API for Redis Modules and Redis Module Utils

## module.c
* [RedisModule_FreeCallReply](#redismodule_freecallreply)

* [RedisModule_CloseKey](#redismodule_closekey)

* [RedisModule_ZsetRangeStop](#redismodule_zsetrangestop)

* [RedisModule_GetApi](#redismodule_getapi)

* [RedisModule_IsKeysPositionRequest](#redismodule_iskeyspositionrequest)

* [RedisModule_KeyAtPos](#redismodule_keyatpos)

* [RedisModule_CreateCommand](#redismodule_createcommand)

* [RedisModule_SetModuleAttribs](#redismodule_setmoduleattribs)

* [RedisModule_AutoMemory](#redismodule_automemory)

* [RedisModule_CreateString](#redismodule_createstring)

* [RedisModule_CreateStringFromLongLong](#redismodule_createstringfromlonglong)

* [RedisModule_FreeString](#redismodule_freestring)

* [RedisModule_StringPtrLen](#redismodule_stringptrlen)

* [RedisModule_StringToLongLong](#redismodule_stringtolonglong)

* [RedisModule_StringToDouble](#redismodule_stringtodouble)

* [RedisModule_WrongArity](#redismodule_wrongarity)

* [RedisModule_ReplyWithLongLong](#redismodule_replywithlonglong)

* [RedisModule_ReplyWithError](#redismodule_replywitherror)

* [RedisModule_ReplyWithSimpleString](#redismodule_replywithsimplestring)

* [RedisModule_ReplyWithArray](#redismodule_replywitharray)

* [RedisModule_ReplySetArrayLength](#redismodule_replysetarraylength)

* [RedisModule_ReplyWithStringBuffer](#redismodule_replywithstringbuffer)

* [RedisModule_ReplyWithString](#redismodule_replywithstring)

* [RedisModule_ReplyWithNull](#redismodule_replywithnull)

* [RedisModule_ReplyWithCallReply](#redismodule_replywithcallreply)

* [RedisModule_ReplyWithDouble](#redismodule_replywithdouble)

* [RedisModule_Replicate](#redismodule_replicate)

* [RedisModule_ReplicateVerbatim](#redismodule_replicateverbatim)

* [RedisModule_GetClientId](#redismodule_getclientid)

* [RedisModule_GetSelectedDb](#redismodule_getselecteddb)

* [RedisModule_SelectDb](#redismodule_selectdb)

* [RedisModule_OpenKey](#redismodule_openkey)

* [RedisModule_CloseKey](#redismodule_closekey)

* [RedisModule_KeyType](#redismodule_keytype)

* [RedisModule_ValueLength](#redismodule_valuelength)

* [RedisModule_DeleteKey](#redismodule_deletekey)

* [RedisModule_GetExpire](#redismodule_getexpire)

* [RedisModule_SetExpire](#redismodule_setexpire)

* [RedisModule_StringSet](#redismodule_stringset)

* [RedisModule_StringDMA](#redismodule_stringdma)

* [RedisModule_StringTruncate](#redismodule_stringtruncate)

* [RedisModule_ListPush](#redismodule_listpush)

* [RedisModule_ListPop](#redismodule_listpop)

* [RedisModule_ZsetAddFlagsToCoreFlags](#redismodule_zsetaddflagstocoreflags)

* [RedisModule_ZsetAddFlagsFromCoreFlags](#redismodule_zsetaddflagsfromcoreflags)

* [RedisModule_ZsetAdd](#redismodule_zsetadd)

* [RedisModule_ZsetIncrby](#redismodule_zsetincrby)

* [RedisModule_ZsetRem](#redismodule_zsetrem)

* [RedisModule_ZsetScore](#redismodule_zsetscore)

* [RedisModule_ZsetRangeStop](#redismodule_zsetrangestop)

* [RedisModule_ZsetRangeEndReached](#redismodule_zsetrangeendreached)

* [RedisModule_ZsetFirstInScoreRange](#redismodule_zsetfirstinscorerange)

* [RedisModule_ZsetLastInScoreRange](#redismodule_zsetlastinscorerange)

* [RedisModule_ZsetFirstInLexRange](#redismodule_zsetfirstinlexrange)

* [RedisModule_ZsetLastInLexRange](#redismodule_zsetlastinlexrange)

* [RedisModule_ZsetRangeCurrentElement](#redismodule_zsetrangecurrentelement)

* [RedisModule_ZsetRangeNext](#redismodule_zsetrangenext)

* [RedisModule_ZsetRangePrev](#redismodule_zsetrangeprev)

* [RedisModule_HashSet](#redismodule_hashset)

* [RedisModule_HashGet](#redismodule_hashget)

* [RedisModule_FreeCallReply_Rec](#redismodule_freecallreply_rec)

* [RedisModule_FreeCallReply](#redismodule_freecallreply)

* [RedisModule_CallReplyType](#redismodule_callreplytype)

* [RedisModule_CallReplyLength](#redismodule_callreplylength)

* [RedisModule_CallReplyArrayElement](#redismodule_callreplyarrayelement)

* [RedisModule_CallReplyInteger](#redismodule_callreplyinteger)

* [RedisModule_CallReplyStringPtr](#redismodule_callreplystringptr)

* [RedisModule_CreateStringFromCallReply](#redismodule_createstringfromcallreply)

* [RedisModule_Call](#redismodule_call)

* [RedisModule_CallReplyProto](#redismodule_callreplyproto)

## util.h
* [RMUtil_ArgExists](#rmutil_argexists)

* [RMUtil_ParseArgs](#rmutil_parseargs)

* [RMUtil_ParseArgsAfter](#rmutil_parseargsafter)

* [RMUtil_GetRedisInfo](#rmutil_getredisinfo)

## strings.h
* [RMUtil_CreateFormattedString](#rmutil_createformattedstring)

* [RMUtil_StringEquals](#rmutil_stringequals)

* [RMUtil_StringToLower](#rmutil_stringtolower)

* [RMUtil_StringToUpper](#rmutil_stringtoupper)

## vector.h
* [Vector_Get](#vector_get)

* [Vector_Resize](#vector_resize)

* [Vector_Size](#vector_size)

* [Vector_Cap](#vector_cap)

* [Vector_Free](#vector_free)
### RedisModule_FreeCallReply
```
void RedisModule_FreeCallReply(RedisModuleCallReply *reply);
```
 --------------------------------------------------------------------------
 Prototypes
 -------------------------------------------------------------------------- 


### RedisModule_CloseKey
```
void RedisModule_CloseKey(RedisModuleKey *key);
```
 --------------------------------------------------------------------------
 Prototypes
 -------------------------------------------------------------------------- 


### RedisModule_ZsetRangeStop
```
void RedisModule_ZsetRangeStop(RedisModuleKey *key);
```
 --------------------------------------------------------------------------
 Prototypes
 -------------------------------------------------------------------------- 


### RedisModule_GetApi
```
int RedisModule_GetApi(const char *funcname, void **targetPtrPtr) {
```
 Lookup the requested module API and store the function pointer into the
 target pointer. The function returns REDISMODULE_ERR if there is no such
 named API, otherwise REDISMODULE_OK.

 This function is not meant to be used by modules developer, it is only
 used implicitly by including redismodule.h. 


### RedisModule_IsKeysPositionRequest
```
int RedisModule_IsKeysPositionRequest(RedisModuleCtx *ctx) {
```
 Return non-zero if a module command, that was declared with the
 flag "getkeys-api", is called in a special way to get the keys positions
 and not to get executed. Otherwise zero is returned. 


### RedisModule_KeyAtPos
```
void RedisModule_KeyAtPos(RedisModuleCtx *ctx, int pos) {
```
 When a module command is called in order to obtain the position of
 keys, since it was flagged as "getkeys-api" during the registration,
 the command implementation checks for this special call using the
 RedisModule_IsKeysPositionRequest() API and uses this function in
 order to report keys, like in the following example:

  if (RedisModule_IsKeysPositionRequest(ctx)) {
      RedisModule_KeyAtPos(ctx,1);
      RedisModule_KeyAtPos(ctx,2);
  }

  Note: in the example below the get keys API would not be needed since
  keys are at fixed positions. This interface is only used for commands
  with a more complex structure. 


### RedisModule_CreateCommand
```
int RedisModule_CreateCommand(RedisModuleCtx *ctx, const char *name, RedisModuleCmdFunc cmdfunc, const char *strflags, int firstkey, int lastkey, int keystep) {
```
 Register a new command in the Redis server, that will be handled by
 calling the function pointer 'func' using the RedisModule calling
 convention. The function returns REDISMODULE_ERR if the specified command
 name is already busy or a set of invalid flags were passed, otherwise
 REDISMODULE_OK is returned and the new command is registered.

 This function must be called during the initialization of the module
 inside the RedisModule_OnLoad() function. Calling this function outside
 of the initialization function is not defined.

 The command function type is the following:

  int MyCommand_RedisCommand(RedisModuleCtx *ctx, RedisModuleString **argv, int argc);

 And is supposed to always return REDISMODULE_OK.

 The set of flags 'strflags' specify the behavior of the command, and should
 be passed as a C string compoesd of space separated words, like for
 example "write deny-oom". The set of flags are:

 * **"write"**:     The command may modify the data set (it may also read
                    from it).
 * **"readonly"**:  The command returns data from keys but never writes.
 * **"admin"**:     The command is an administrative command (may change
                    replication or perform similar tasks).
 * **"deny-oom"**:  The command may use additional memory and should be
                    denied during out of memory conditions.
 * **"deny-script"**:   Don't allow this command in Lua scripts.
 * **"allow-loading"**: Allow this command while the server is loading data.
                        Only commands not interacting with the data set
                        should be allowed to run in this mode. If not sure
                        don't use this flag.
 * **"pubsub"**:    The command publishes things on Pub/Sub channels.
 * **"random"**:    The command may have different outputs even starting
                    from the same input arguments and key values.
 * **"allow-stale"**: The command is allowed to run on slaves that don't
                      serve stale data. Don't use if you don't know what
                      this means.
 * **"no-monitor"**: Don't propoagate the command on monitor. Use this if
                     the command has sensible data among the arguments.
 * **"fast"**:      The command time complexity is not greater
                    than O(log(N)) where N is the size of the collection or
                    anything else representing the normal scalability
                    issue with the command.
 * **"getkeys-api"**: The command implements the interface to return
                      the arguments that are keys. Used when start/stop/step
                      is not enough because of the command syntax.
 * **"no-cluster"**: The command should not register in Redis Cluster
                     since is not designed to work with it because, for
                     example, is unable to report the position of the
                     keys, programmatically creates key names, or any
                     other reason.


### RedisModule_SetModuleAttribs
```
void RedisModule_SetModuleAttribs(RedisModuleCtx *ctx, const char *name, int ver, int apiver){
```
 Called by RM_Init() to setup the ctx->module structure.

 This is an internal function, Redis modules developers don't need
 to use it. 


### RedisModule_AutoMemory
```
void RedisModule_AutoMemory(RedisModuleCtx *ctx) {
```
 Enable automatic memory management. See API.md for more information.

 The function must be called as the first function of a command implementation
 that wants to use automatic memory. 


### RedisModule_CreateString
```
RedisModuleString *RedisModule_CreateString(RedisModuleCtx *ctx, const char *ptr, size_t len)
```
 Create a new module string object. The returned string must be freed
 with RedisModule_FreeString(), unless automatic memory is enabled.

 The string is created by copying the `len` bytes starting
 at `ptr`. No reference is retained to the passed buffer. 


### RedisModule_CreateStringFromLongLong
```
RedisModuleString *RedisModule_CreateStringFromLongLong(RedisModuleCtx *ctx, long long ll) {
```
 Like RedisModule_CreatString(), but creates a string starting from a long long
 integer instead of taking a buffer and its length.

 The returned string must be released with RedisModule_FreeString() or by
 enabling automatic memory management. 


### RedisModule_FreeString
```
void RedisModule_FreeString(RedisModuleCtx *ctx, RedisModuleString *str) {
```
 Free a module string object obtained with one of the Redis modules API calls
 that return new string objects.

 It is possible to call this function even when automatic memory management
 is enabled. In that case the string will be released ASAP and removed
 from the pool of string to release at the end. 


### RedisModule_StringPtrLen
```
const char *RedisModule_StringPtrLen(RedisModuleString *str, size_t *len) {
```
 Given a string module object, this function returns the string pointer
 and length of the string. The returned pointer and length should only
 be used for read only accesses and never modified. 


### RedisModule_StringToLongLong
```
int RedisModule_StringToLongLong(RedisModuleString *str, long long *ll) {
```
 Convert the string into a long long integer, storing it at *ll.
 Returns REDISMODULE_OK on success. If the string can't be parsed
 as a valid, strict long long (no spaces before/after), REDISMODULE_ERR
 is returned. 


### RedisModule_StringToDouble
```
int RedisModule_StringToDouble(RedisModuleString *str, double *d) {
```
 Convert the string into a double, storing it at *d.
 Returns REDISMODULE_OK on success or REDISMODULE_ERR if the string is
 not a valid string representation of a double value. 


### RedisModule_WrongArity
```
int RedisModule_WrongArity(RedisModuleCtx *ctx) {
```
 Send an error about the number of arguments given to the command,
 citing the command name in the error message.

 Example:

  if (argc != 3) return RedisModule_WrongArity(ctx);


### RedisModule_ReplyWithLongLong
```
int RedisModule_ReplyWithLongLong(RedisModuleCtx *ctx, long long ll) {
```
 Send an integer reply to the client, with the specified long long value.
 The function always returns REDISMODULE_OK. 


### RedisModule_ReplyWithError
```
int RedisModule_ReplyWithError(RedisModuleCtx *ctx, const char *err) {
```
 Reply with the error 'err'.

 Note that 'err' must contain all the error, including
 the initial error code. The function only provides the initial "-", so
 the usage is, for example:

  RM_ReplyWithError(ctx,"ERR Wrong Type");

 and not just:

  RM_ReplyWithError(ctx,"Wrong Type");

 The function always returns REDISMODULE_OK.


### RedisModule_ReplyWithSimpleString
```
int RedisModule_ReplyWithSimpleString(RedisModuleCtx *ctx, const char *msg) {
```
 Reply with a simple string (+... \r\n in RESP protocol). This replies
 are suitable only when sending a small non-binary string with small
 overhead, like "OK" or similar replies.

 The function always returns REDISMODULE_OK. 


### RedisModule_ReplyWithArray
```
int RedisModule_ReplyWithArray(RedisModuleCtx *ctx, long len) {
```
 Reply with an array type of 'len' elements. However 'len' other calls
 to ReplyWith* style functions must follow in order to emit the elements
 of the array.

 When producing arrays with a number of element that is not known beforehand
 the function can be called with the special count
 REDISMODULE_POSTPONED_ARRAY_LEN, and the actual number of elements can be
 later set with RedisModule_ReplySetArrayLength() (which will set the
 latest "open" count if there are multiple ones).

 The function always returns REDISMODULE_OK. 


### RedisModule_ReplySetArrayLength
```
void RedisModule_ReplySetArrayLength(RedisModuleCtx *ctx, long len) {
```
 When RedisModule_ReplyWithArray() is used with the argument
 REDISMODULE_POSTPONED_ARRAY_LEN, because we don't know beforehand the number
 of items we are going to output as elements of the array, this function
 will take care to set the array length.

 Since it is possible to have multiple array replies pending with unknown
 length, this function guarantees to always set the latest array length
 that was created in a postponed way.

 For example in order to output an array like [1,[10,20,30]] we
 could write:

  RedisModule_ReplyWithArray(ctx,REDISMODULE_POSTPONED_ARRAY_LEN);
  RedisModule_ReplyWithLongLong(ctx,1);
  RedisModule_ReplyWithArray(ctx,REDISMODULE_POSTPONED_ARRAY_LEN);
  RedisModule_ReplyWithLongLong(ctx,10);
  RedisModule_ReplyWithLongLong(ctx,20);
  RedisModule_ReplyWithLongLong(ctx,30);
  RedisModule_ReplySetArrayLength(ctx,3); // Set len of 10,20,30 array.
  RedisModule_ReplySetArrayLength(ctx,2); // Set len of top array

 Note that in the above example there is no reason to postpone the array
 length, since we produce a fixed number of elements, but in the practice
 the code may use an interator or other ways of creating the output so
 that is not easy to calculate in advance the number of elements.


### RedisModule_ReplyWithStringBuffer
```
int RedisModule_ReplyWithStringBuffer(RedisModuleCtx *ctx, const char *buf, size_t len) {
```
 Reply with a bulk string, taking in input a C buffer pointer and length.

 The function always returns REDISMODULE_OK. 


### RedisModule_ReplyWithString
```
int RedisModule_ReplyWithString(RedisModuleCtx *ctx, RedisModuleString *str) {
```
 Reply with a bulk string, taking in input a RedisModuleString object.

 The function always returns REDISMODULE_OK. 


### RedisModule_ReplyWithNull
```
int RedisModule_ReplyWithNull(RedisModuleCtx *ctx) {
```
 Reply to the client with a NULL. In the RESP protocol a NULL is encoded
 as the string "$-1\r\n".

 The function always returns REDISMODULE_OK. 


### RedisModule_ReplyWithCallReply
```
int RedisModule_ReplyWithCallReply(RedisModuleCtx *ctx, RedisModuleCallReply *reply) {
```
 Reply exactly what a Redis command returned us with RedisModule_Call().
 This function is useful when we use RedisModule_Call() in order to
 execute some command, as we want to reply to the client exactly the
 same reply we obtained by the command.

 The function always returns REDISMODULE_OK. 


### RedisModule_ReplyWithDouble
```
int RedisModule_ReplyWithDouble(RedisModuleCtx *ctx, double d) {
```
 Send a string reply obtained converting the double 'd' into a bulk string.
 This function is basically equivalent to converting a double into
 a string into a C buffer, and then calling the function
 RedisModule_ReplyWithStringBuffer() with the buffer and length.

 The function always returns REDISMODULE_OK. 


### RedisModule_Replicate
```
int RedisModule_Replicate(RedisModuleCtx *ctx, const char *cmdname, const char *fmt, ...) {
```
 Replicate the specified command and arguments to slaves and AOF, as effect
 of execution of the calling command implementation.

 The replicated commands are always wrapped into the MULTI/EXEC that
 contains all the commands replicated in a given module command
 execution. However the commands replicated with RedisModule_Call()
 are the first items, the ones replicated with RedisModule_Replicate()
 will all follow before the EXEC.

 Modules should try to use one interface or the other.

 This command follows exactly the same interface of RedisModule_Call(),
 so a set of format specifiers must be passed, followed by arguments
 matching the provided format specifiers.

 Please refer to RedisModule_Call() for more information.

 The command returns REDISMODULE_ERR if the format specifiers are invalid
 or the command name does not belong to a known command. 


### RedisModule_ReplicateVerbatim
```
int RedisModule_ReplicateVerbatim(RedisModuleCtx *ctx) {
```
 This function will replicate the command exactly as it was invoked
 by the client. Note that this function will not wrap the command into
 a MULTI/EXEC stanza, so it should not be mixed with other replication
 commands.

 Basically this form of replication is useful when you want to propagate
 the command to the slaves and AOF file exactly as it was called, since
 the command can just be re-executed to deterministically re-create the
 new state starting from the old one.

 The function always returns REDISMODULE_OK. 


### RedisModule_GetClientId
```
unsigned long long RedisModule_GetClientId(RedisModuleCtx *ctx) {
```
 Return the ID of the current client calling the currently active module
 command. The returned ID has a few guarantees:

 1. The ID is different for each different client, so if the same client
    executes a module command multiple times, it can be recognized as
    having the same ID, otherwise the ID will be different.
 2. The ID increases monotonically. Clients connecting to the server later
    are guaranteed to get IDs greater than any past ID previously seen.

 Valid IDs are from 1 to 2^64-1. If 0 is returned it means there is no way
 to fetch the ID in the context the function was currently called. 


### RedisModule_GetSelectedDb
```
int RedisModule_GetSelectedDb(RedisModuleCtx *ctx) {
```
 Return the currently selected DB. 


### RedisModule_SelectDb
```
int RedisModule_SelectDb(RedisModuleCtx *ctx, int newid) {
```
 Change the currently selected DB. Returns an error if the id
 is out of range.

 Note that the client will retain the currently selected DB even after
 the Redis command implemented by the module calling this function
 returns.

 If the module command wishes to change something in a different DB and
 returns back to the original one, it should call RedisModule_GetSelectedDb()
 before in order to restore the old DB number before returning. 


### RedisModule_OpenKey
```
void *RedisModule_OpenKey(RedisModuleCtx *ctx, robj *keyname, int mode) {
```
 Return an handle representing a Redis key, so that it is possible
 to call other APIs with the key handle as argument to perform
 operations on the key.

 The return value is the handle repesenting the key, that must be
 closed with RM_CloseKey().

 If the key does not exist and WRITE mode is requested, the handle
 is still returned, since it is possible to perform operations on
 a yet not existing key (that will be created, for example, after
 a list push operation). If the mode is just READ instead, and the
 key does not exist, NULL is returned. However it is still safe to
 call RedisModule_CloseKey() and RedisModule_KeyType() on a NULL
 value. 


### RedisModule_CloseKey
```
void RedisModule_CloseKey(RedisModuleKey *key) {
```
 Close a key handle. 


### RedisModule_KeyType
```
int RedisModule_KeyType(RedisModuleKey *key) {
```
 Return the type of the key. If the key pointer is NULL then
 REDISMODULE_KEYTYPE_EMPTY is returned. 


### RedisModule_ValueLength
```
size_t RedisModule_ValueLength(RedisModuleKey *key) {
```
 Return the length of the value associated with the key.
 For strings this is the length of the string. For all the other types
 is the number of elements (just counting keys for hashes).

 If the key pointer is NULL or the key is empty, zero is returned. 


### RedisModule_DeleteKey
```
int RedisModule_DeleteKey(RedisModuleKey *key) {
```
 If the key is open for writing, remove it, and setup the key to
 accept new writes as an empty key (that will be created on demand).
 On success REDISMODULE_OK is returned. If the key is not open for
 writing REDISMODULE_ERR is returned. 


### RedisModule_GetExpire
```
mstime_t RedisModule_GetExpire(RedisModuleKey *key) {
```
 Return the key expire value, as milliseconds of remaining TTL.
 If no TTL is associated with the key or if the key is empty,
 REDISMODULE_NO_EXPIRE is returned. 


### RedisModule_SetExpire
```
int RedisModule_SetExpire(RedisModuleKey *key, mstime_t expire) {
```
 Set a new expire for the key. If the special expire
 REDISMODULE_NO_EXPIRE is set, the expire is cancelled if there was
 one (the same as the PERSIST command).

 Note that the expire must be provided as a positive integer representing
 the number of milliseconds of TTL the key should have.

 The function returns REDISMODULE_OK on success or REDISMODULE_ERR if
 the key was not open for writing or is an empty key. 


### RedisModule_StringSet
```
int RedisModule_StringSet(RedisModuleKey *key, RedisModuleString *str) {
```
 If the key is open for writing, set the specified string 'str' as the
 value of the key, deleting the old value if any.
 On success REDISMODULE_OK is returned. If the key is not open for
 writing or there is an active iterator, REDISMODULE_ERR is returned. 


### RedisModule_StringDMA
```
char *RedisModule_StringDMA(RedisModuleKey *key, size_t *len, int mode) {
```
 Prepare the key associated string value for DMA access, and returns
 a pointer and size (by reference), that the user can use to read or
 modify the string in-place accessing it directly via pointer.

 The 'mode' is composed by bitwise OR-ing the following flags:

 REDISMODULE_READ -- Read access
 REDISMODULE_WRITE -- Write access

 If the DMA is not requested for writing, the pointer returned should
 only be accessed in a read-only fashion.

 On error (wrong type) NULL is returned.

 DMA access rules:

 1. No other key writing function should be called since the moment
 the pointer is obtained, for all the time we want to use DMA access
 to read or modify the string.

 2. Each time RM_StringTruncate() is called, to continue with the DMA
 access, RM_StringDMA() should be called again to re-obtain
 a new pointer and length.

 3. If the returned pointer is not NULL, but the length is zero, no
 byte can be touched (the string is empty, or the key itself is empty)
 so a RM_StringTruncate() call should be used if there is to enlarge
 the string, and later call StringDMA() again to get the pointer.


### RedisModule_StringTruncate
```
int RedisModule_StringTruncate(RedisModuleKey *key, size_t newlen) {
```
 If the string is open for writing and is of string type, resize it, padding
 with zero bytes if the new length is greater than the old one.

 After this call, RM_StringDMA() must be called again to continue
 DMA access with the new pointer.

 The function returns REDISMODULE_OK on success, and REDISMODULE_ERR on
 error, that is, the key is not open for writing, is not a string
 or resizing for more than 512 MB is requested.

 If the key is empty, a string key is created with the new string value
 unless the new length value requested is zero. 


### RedisModule_ListPush
```
int RedisModule_ListPush(RedisModuleKey *key, int where, RedisModuleString *ele) {
```
 Push an element into a list, on head or tail depending on 'where' argumnet.
 If the key pointer is about an empty key opened for writing, the key
 is created. On error (key opened for read-only operations or of the wrong
 type) REDISMODULE_ERR is returned, otherwise REDISMODULE_OK is returned. 


### RedisModule_ListPop
```
RedisModuleString *RedisModule_ListPop(RedisModuleKey *key, int where) {
```
 Pop an element from the list, and returns it as a module string object
 that the user should be free with RM_FreeString() or by enabling
 automatic memory. 'where' specifies if the element should be popped from
 head or tail. The command returns NULL if:
 1) The list is empty.
 2) The key was not open for writing.
 3) The key is not a list. 


### RedisModule_ZsetAddFlagsToCoreFlags
```
int RedisModule_ZsetAddFlagsToCoreFlags(int flags) {
```
 Conversion from/to public flags of the Modules API and our private flags,
 so that we have everything decoupled. 


### RedisModule_ZsetAddFlagsFromCoreFlags
```
int RedisModule_ZsetAddFlagsFromCoreFlags(int flags) {
```
 See previous function comment. 


### RedisModule_ZsetAdd
```
int RedisModule_ZsetAdd(RedisModuleKey *key, double score, RedisModuleString *ele, int *flagsptr) {
```
 Add a new element into a sorted set, with the specified 'score'.
 If the element already exists, the score is updated.

 A new sorted set is created at value if the key is an empty open key
 setup for writing.

 Additional flags can be passed to the function via a pointer, the flags
 are both used to receive input and to communicate state when the function
 returns. 'flagsptr' can be NULL if no special flags are used.

 The input flags are:

 REDISMODULE_ZADD_XX: Element must already exist. Do nothing otherwise.
 REDISMODULE_ZADD_NX: Element must not exist. Do nothing otherwise.

 The output flags are:

 REDISMODULE_ZADD_ADDED: The new element was added to the sorted set.
 REDISMODULE_ZADD_UPDATED: The score of the element was updated.
 REDISMODULE_ZADD_NOP: No operation was performed because XX or NX flags.

 On success the function returns REDISMODULE_OK. On the following errors
 REDISMODULE_ERR is returned:

 - The key was not opened for writing.
 - The key is of the wrong type.
 - 'score' double value is not a number (NaN).


### RedisModule_ZsetIncrby
```
int RedisModule_ZsetIncrby(RedisModuleKey *key, double score, RedisModuleString *ele, int *flagsptr, double *newscore) {
```
 This function works exactly like RM_ZsetAdd(), but instead of setting
 a new score, the score of the existing element is incremented, or if the
 element does not already exist, it is added assuming the old score was
 zero.

 The input and output flags, and the return value, have the same exact
 meaning, with the only difference that this function will return
 REDISMODULE_ERR even when 'score' is a valid double number, but adding it
 to the existing score resuts into a NaN (not a number) condition.

 This function has an additional field 'newscore', if not NULL is filled
 with the new score of the element after the increment, if no error
 is returned. 


### RedisModule_ZsetRem
```
int RedisModule_ZsetRem(RedisModuleKey *key, RedisModuleString *ele, int *deleted) {
```
 Remove the specified element from the sorted set.
 The function returns REDISMODULE_OK on success, and REDISMODULE_ERR
 on one of the following conditions:

 - The key was not opened for writing.
 - The key is of the wrong type.

 The return value does NOT indicate the fact the element was really
 removed (since it existed) or not, just if the function was executed
 with success.

 In order to know if the element was removed, the additional argument
 'deleted' must be passed, that populates the integer by reference
 setting it to 1 or 0 depending on the outcome of the operation.
 The 'deleted' argument can be NULL if the caller is not interested
 to know if the element was really removed.

 Empty keys will be handled correctly by doing nothing. 


### RedisModule_ZsetScore
```
int RedisModule_ZsetScore(RedisModuleKey *key, RedisModuleString *ele, double *score) {
```
 On success retrieve the double score associated at the sorted set element
 'ele' and returns REDISMODULE_OK. Otherwise REDISMODULE_ERR is returned
 to signal one of the following conditions:

 - There is no such element 'ele' in the sorted set.
 - The key is not a sorted set.
 - The key is an open empty key.


### RedisModule_ZsetRangeStop
```
void RedisModule_ZsetRangeStop(RedisModuleKey *key) {
```
 Stop a sorted set iteration. 


### RedisModule_ZsetRangeEndReached
```
int RedisModule_ZsetRangeEndReached(RedisModuleKey *key) {
```
 Return the "End of range" flag value to signal the end of the iteration. 


### RedisModule_ZsetFirstInScoreRange
```
int RedisModule_ZsetFirstInScoreRange(RedisModuleKey *key, double min, double max, int minex, int maxex) {
```
 Setup a sorted set iterator seeking the first element in the specified
 range. Returns REDISMODULE_OK if the iterator was correctly initialized
 otherwise REDISMODULE_ERR is returned in the following conditions:

 1. The value stored at key is not a sorted set or the key is empty.

 The range is specified according to the two double values 'min' and 'max'.
 Both can be infinite using the following two macros:

 REDISMODULE_POSITIVE_INFINITE for positive infinite value
 REDISMODULE_NEGATIVE_INFINITE for negative infinite value

 'minex' and 'maxex' parameters, if true, respectively setup a range
 where the min and max value are exclusive (not included) instead of
 inclusive. 


### RedisModule_ZsetLastInScoreRange
```
int RedisModule_ZsetLastInScoreRange(RedisModuleKey *key, double min, double max, int minex, int maxex) {
```
 Exactly like RedisModule_ZsetFirstInScoreRange() but the last element of
 the range is selected for the start of the iteration instead. 


### RedisModule_ZsetFirstInLexRange
```
int RedisModule_ZsetFirstInLexRange(RedisModuleKey *key, RedisModuleString *min, RedisModuleString *max) {
```
 Setup a sorted set iterator seeking the first element in the specified
 lexicographical range. Returns REDISMODULE_OK if the iterator was correctly
 initialized otherwise REDISMODULE_ERR is returned in the
 following conditions:

 1. The value stored at key is not a sorted set or the key is empty.
 2. The lexicographical range 'min' and 'max' format is invalid.

 'min' and 'max' should be provided as two RedisModuleString objects
 in the same format as the parameters passed to the ZRANGEBYLEX command.
 The function does not take ownership of the objects, so they can be released
 ASAP after the iterator is setup. 


### RedisModule_ZsetLastInLexRange
```
int RedisModule_ZsetLastInLexRange(RedisModuleKey *key, RedisModuleString *min, RedisModuleString *max) {
```
 Exactly like RedisModule_ZsetFirstInLexRange() but the last element of
 the range is selected for the start of the iteration instead. 


### RedisModule_ZsetRangeCurrentElement
```
RedisModuleString *RedisModule_ZsetRangeCurrentElement(RedisModuleKey *key, double *score) {
```
 Return the current sorted set element of an active sorted set iterator
 or NULL if the range specified in the iterator does not include any
 element. 


### RedisModule_ZsetRangeNext
```
int RedisModule_ZsetRangeNext(RedisModuleKey *key) {
```
 Go to the next element of the sorted set iterator. Returns 1 if there was
 a next element, 0 if we are already at the latest element or the range
 does not include any item at all. 


### RedisModule_ZsetRangePrev
```
int RedisModule_ZsetRangePrev(RedisModuleKey *key) {
```
 Go to the previous element of the sorted set iterator. Returns 1 if there was
 a previous element, 0 if we are already at the first element or the range
 does not include any item at all. 


### RedisModule_HashSet
```
int RedisModule_HashSet(RedisModuleKey *key, int flags, ...) {
```
 Set the field of the specified hash field to the specified value.
 If the key is an empty key open for writing, it is created with an empty
 hash value, in order to set the specified field.

 The function is variadic and the user must specify pairs of field
 names and values, both as RedisModuleString pointers (unless the
 CFIELD option is set, see later).

 Example to set the hash argv[1] to the value argv[2]:

  RedisModule_HashSet(key,REDISMODULE_HASH_NONE,argv[1],argv[2],NULL);

 The function can also be used in order to delete fields (if they exist)
 by setting them to the specified value of REDISMODULE_HASH_DELETE:

  RedisModule_HashSet(key,REDISMODULE_HASH_NONE,argv[1],
                      REDISMODULE_HASH_DELETE,NULL);

 The behavior of the command changes with the specified flags, that can be
 set to REDISMODULE_HASH_NONE if no special behavior is needed.

 REDISMODULE_HASH_NX: The operation is performed only if the field was not
                     already existing in the hash.
 REDISMODULE_HASH_XX: The operation is performed only if the field was
                     already existing, so that a new value could be
                     associated to an existing filed, but no new fields
                     are created.
 REDISMODULE_HASH_CFIELDS: The field names passed are null terminated C
                          strings instead of RedisModuleString objects.

 Unless NX is specified, the command overwrites the old field value with
 the new one.

 When using REDISMODULE_HASH_CFIELDS, field names are reported using
 normal C strings, so for example to delete the field "foo" the following
 code can be used:

  RedisModule_HashSet(key,REDISMODULE_HASH_CFIELDS,"foo",
                      REDISMODULE_HASH_DELETE,NULL);

 Return value:

 The number of fields updated (that may be less than the number of fields
 specified because of the XX or NX options).

 In the following case the return value is always zero:

 - The key was not open for writing.
 - The key was associated with a non Hash value.


### RedisModule_HashGet
```
int RedisModule_HashGet(RedisModuleKey *key, int flags, ...) {
```
 Get fields from an hash value. This function is called using a variable
 number of arguments, alternating a field name (as a StringRedisModule
 pointer) with a pointer to a StringRedisModule pointer, that is set to the
 value of the field if the field exist, or NULL if the field did not exist.
 At the end of the field/value-ptr pairs, NULL must be specified as last
 argument to signal the end of the arguments in the variadic function.

 This is an example usage:

  RedisModuleString *first, *second;
  RedisModule_HashGet(mykey,REDISMODULE_HASH_NONE,argv[1],&first,
                      argv[2],&second,NULL);

 As with RedisModule_HashSet() the behavior of the command can be specified
 passing flags different than REDISMODULE_HASH_NONE:

 REDISMODULE_HASH_CFIELD: field names as null terminated C strings.

 REDISMODULE_HASH_EXISTS: instead of setting the value of the field
 expecting a RedisModuleString pointer to pointer, the function just
 reports if the field esists or not and expects an integer pointer
 as the second element of each pair.

 Example of REDISMODULE_HASH_CFIELD:

  RedisModuleString *username, *hashedpass;
  RedisModule_HashGet(mykey,"username",&username,"hp",&hashedpass, NULL);

 Example of REDISMODULE_HASH_EXISTS:

  int exists;
  RedisModule_HashGet(mykey,argv[1],&exists,NULL);

 The function returns REDISMODULE_OK on success and REDISMODULE_ERR if
 the key is not an hash value.

 Memory management:

 The returned RedisModuleString objects should be released with
 RedisModule_FreeString(), or by enabling automatic memory management.


### RedisModule_FreeCallReply_Rec
```
void RedisModule_FreeCallReply_Rec(RedisModuleCallReply *reply, int freenested){
```
 Free a Call reply and all the nested replies it contains if it's an
 array. 


### RedisModule_FreeCallReply
```
void RedisModule_FreeCallReply(RedisModuleCallReply *reply) {
```
 Wrapper for the recursive free reply function. This is needed in order
 to have the first level function to return on nested replies, but only
 if called by the module API. 


### RedisModule_CallReplyType
```
int RedisModule_CallReplyType(RedisModuleCallReply *reply) {
```
 Return the reply type. 


### RedisModule_CallReplyLength
```
size_t RedisModule_CallReplyLength(RedisModuleCallReply *reply) {
```
 Return the reply type length, where applicable. 


### RedisModule_CallReplyArrayElement
```
RedisModuleCallReply *RedisModule_CallReplyArrayElement(RedisModuleCallReply *reply, size_t idx) {
```
 Return the 'idx'-th nested call reply element of an array reply, or NULL
 if the reply type is wrong or the index is out of range. 


### RedisModule_CallReplyInteger
```
long long RedisModule_CallReplyInteger(RedisModuleCallReply *reply) {
```
 Return the long long of an integer reply. 


### RedisModule_CallReplyStringPtr
```
const char *RedisModule_CallReplyStringPtr(RedisModuleCallReply *reply, size_t *len) {
```
 Return the pointer and length of a string or error reply. 


### RedisModule_CreateStringFromCallReply
```
RedisModuleString *RedisModule_CreateStringFromCallReply(RedisModuleCallReply *reply) {
```
 Return a new string object from a call reply of type string, error or
 integer. Otherwise (wrong reply type) return NULL. 


### RedisModule_Call
```
RedisModuleCallReply *RedisModule_Call(RedisModuleCtx *ctx, const char *cmdname, const char *fmt, ...) {
```
 Exported API to call any Redis command from modules.
 On success a RedisModuleCallReply object is returned, otherwise
 NULL is returned and errno is set to the following values:

 EINVAL: command non existing, wrong arity, wrong format specifier.
 EPERM:  operation in Cluster instance with key in non local slot. 


### RedisModule_CallReplyProto
```
const char *RedisModule_CallReplyProto(RedisModuleCallReply *reply, size_t *len) {
```
 Return a pointer, and a length, to the protocol returned by the command
 that returned the reply object. 


### RMUtil_ArgExists
```
int RMUtil_ArgExists(const char *arg, RedisModuleString **argv, int argc, int offset);
```
 Return the offset of an arg if it exists in the arg list, or 0 if it's not there 


### RMUtil_ParseArgs
```
int RMUtil_ParseArgs(RedisModuleString **argv, int argc, int offset, const char *fmt, ...);
```
Automatically conver the arg list to corresponding variable pointers according to a given format.
You pass it the command arg list and count, the starting offset, a parsing format, and pointers to the variables.
The format is a string consisting of the following identifiers:

    c -- pointer to a Null terminated C string pointer.
    s -- pointer to a RedisModuleString
    l -- pointer to Long long integer.
    d -- pointer to a Double
    * -- do not parse this argument at all
    
Example: If I want to parse args[1], args[2] as a long long and double, I do:
    double d;
    long long l;
    RMUtil_ParseArgs(argv, argc, 1, "ld", &l, &d);


### RMUtil_ParseArgsAfter
```
int RMUtil_ParseArgsAfter(const char *token, RedisModuleString **argv, int argc, const char *fmt, ...);
```
Same as RMUtil_ParseArgs, but only parses the arguments after `token`, if it was found. 
This is useful for optional stuff like [LIMIT [offset] [limit]]


### RMUtil_GetRedisInfo
```
RMUtilInfo *RMUtil_GetRedisInfo(RedisModuleCtx *ctx);
```
 Get redis INFO result and parse it as RMUtilInfo.
 Returns NULL if something goes wrong.
 The resulting object needs to be freed with RMUtilRedisInfo_Free


### RMUtil_CreateFormattedString
```
RedisModuleString *RMUtil_CreateFormattedString(RedisModuleCtx *ctx, const char *fmt, ...);
```
 Create a new RedisModuleString object from a printf-style format and arguments.
 Note that RedisModuleString objects CANNOT be used as formatting arguments.


### RMUtil_StringEquals
```
int RMUtil_StringEquals(RedisModuleString *s1, RedisModuleString *s2);
```
 Return 1 if the two strings are equal. Case *sensitive* 


### RMUtil_StringToLower
```
void RMUtil_StringToLower(RedisModuleString *s);
```
 Converts a redis string to lowercase in place without reallocating anything 


### RMUtil_StringToUpper
```
void RMUtil_StringToUpper(RedisModuleString *s);
```
 Converts a redis string to uppercase in place without reallocating anything 


### Vector_Get
```
int Vector_Get(Vector *v, size_t pos, void *ptr);
```
 get the element at index pos. The value is copied in to ptr. If pos is outside
 the vector capacity, we return 0
 otherwise 1


### Vector_Resize
```
int Vector_Resize(Vector *v, size_t newcap);
```
 resize capacity of v 


### Vector_Size
```
inline int Vector_Size(Vector *v) { return v->top; }
```
 return the used size of the vector, regardless of capacity 


### Vector_Cap
```
inline int Vector_Cap(Vector *v) { return v->cap; }
```
 return the actual capacity 


### Vector_Free
```
void Vector_Free(Vector *v);
```
 free the vector and the underlying data. Does not release its elements if
 they are pointers

