use std::os::raw::c_void;

extern crate libc;

use libc::c_int;

extern crate redismodule;

use redismodule::error::Error;
use redismodule::Command;
use redismodule::raw;
use redismodule::Redis;
use redismodule::raw::module_init;
use redismodule::types::RedisType;

mod redisdoc;

const MODULE_NAME: &str = "redisdoc";
const MODULE_VERSION: c_int = 1;

static DOC_TYPE: RedisType = RedisType::new();

struct DocSetCommand;

impl Command for DocSetCommand {
    fn name() -> &'static str { "doc.set" }

    fn external_command() -> raw::CommandFunc { DocSetCommand_Redis }

    fn str_flags() -> &'static str { "write" }

    // Run the command.
    fn run(r: Redis, args: &[&str]) -> Result<(), Error> {
        if args.len() != 3 {
            // FIXME: Use RedisModule_WrongArity instead. Return an ArityError here and
            // in the low-level implementation call RM_WrongArity.
            return Err(Error::generic(format!(
                "Usage: {} <key> <size>", Self::name()
            ).as_str()));
        }

        // the first argument is command name (ignore it)
        let key = args[1];
        let value = args[2];

        let key = r.open_key_writable(key);
        let key_type = key.verify_and_get_type(&DOC_TYPE)?;

        let my = match key_type {
            raw::KeyType::Empty => {
                Box::new(
                    redisdoc::RedisDoc::from_str(value)?
                )
            }
            _ => {
                // There is an existing value, reuse it
                let my = key.get_value() as *mut redisdoc::RedisDoc;

                if my.is_null() {
                    r.reply_integer(0)?;
                    return Ok(());
                }

                let mut my = unsafe { Box::from_raw(my) };
                my.set_value(value)?;
                my
            }
        };

        let my = Box::into_raw(my);

        key.set_value(&DOC_TYPE, my as *mut c_void)?;

        let size = 0; // TODO: Calc size
        r.reply_integer(size)?;

        Ok(())
    }
}

#[allow(non_snake_case)]
pub extern "C" fn DocSetCommand_Redis(
    ctx: *mut raw::RedisModuleCtx,
    argv: *mut *mut raw::RedisModuleString,
    argc: c_int,
) -> c_int {
    DocSetCommand::execute(ctx, argv, argc).into()
}

//////////////////////////////////////////////////////

struct DocGetCommand;

impl Command for DocGetCommand {
    fn name() -> &'static str { "doc.get" }

    fn external_command() -> raw::CommandFunc { DocGetCommand_Redis }

    fn str_flags() -> &'static str { "" }

    // Run the command.
    fn run(r: Redis, args: &[&str]) -> Result<(), Error> {
        if args.len() != 2 {
            // FIXME: Use RedisModule_WrongArity instead. Return an ArityError here and
            // in the low-level implementation call RM_WrongArity.
            return Err(Error::generic(format!(
                "Usage: {} <key>", Self::name()
            ).as_str()));
        }

        // the first argument is command name (ignore it)
        let key = args[1];

        let key = r.open_key(key);
        key.verify_and_get_type(&DOC_TYPE)?;
        let my = key.get_value() as *mut redisdoc::RedisDoc;

        if my.is_null() {
            r.reply_integer(0)?;
            return Ok(());
        }

        let my = unsafe { &mut *my };
        let size = 42; // TODO // my.data.len();

        r.reply_array(2)?;
        r.reply_integer(size as i64)?;
        r.reply_string(my.to_string()?.as_str())?;

        Ok(())
    }
}

#[allow(non_snake_case)]
pub extern "C" fn DocGetCommand_Redis(
    ctx: *mut raw::RedisModuleCtx,
    argv: *mut *mut raw::RedisModuleString,
    argc: c_int,
) -> c_int {
    DocGetCommand::execute(ctx, argv, argc).into()
}

//////////////////////////////////////////////////////

struct DocDelCommand;

impl Command for DocDelCommand {
    fn name() -> &'static str { "doc.del" }

    fn external_command() -> raw::CommandFunc { DocDelCommand_Redis }

    fn str_flags() -> &'static str { "write" }

    // Run the command.
    fn run(r: Redis, args: &[&str]) -> Result<(), Error> {
        if args.len() != 2 {
            // FIXME: Use RedisModule_WrongArity instead?
            return Err(Error::generic(format!(
                "Usage: {} <key>", Self::name()
            ).as_str()));
        }

        // the first argument is command name (ignore it)
        let _key = args[1];

        r.reply_string("OK")?;

        Ok(())
    }
}

// TODO: Write a macro to generate these glue functions
// TODO: Look at https://github.com/faineance/redismodule which has some macros

#[allow(non_snake_case)]
pub extern "C" fn DocDelCommand_Redis(
    ctx: *mut raw::RedisModuleCtx,
    argv: *mut *mut raw::RedisModuleString,
    argc: c_int,
) -> c_int {
    DocDelCommand::execute(ctx, argv, argc).into()
}

fn module_on_load(ctx: *mut raw::RedisModuleCtx) -> Result<(), &'static str> {
    module_init(ctx, MODULE_NAME, MODULE_VERSION)?;

    // TODO: Call this from inside module_init
    redismodule::use_redis_alloc();

    DOC_TYPE.create_data_type(ctx, "RedisDoc1")?;

    DocSetCommand::create(ctx)?;
    DocGetCommand::create(ctx)?;
    DocDelCommand::create(ctx)?;

    Ok(())
}

#[allow(non_snake_case)]
#[no_mangle]
pub extern "C" fn RedisModule_OnLoad(
    ctx: *mut raw::RedisModuleCtx,
    _argv: *mut *mut raw::RedisModuleString,
    _argc: c_int,
) -> c_int {
    if let Err(msg) = module_on_load(ctx) {
        eprintln!("Error loading module: {}", msg);
        return raw::Status::Err.into();
    }

    raw::Status::Ok.into()
}
