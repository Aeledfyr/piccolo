use crate::{Callback, CallbackReturn, Context, FromValue, String, Table, Value};

mod pattern;

pub fn load_string<'gc>(ctx: Context<'gc>) {
    let string = Table::new(&ctx);

    string.set_field(
        ctx,
        "len",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let string = stack.consume::<String>(ctx)?;
            let len = string.len();
            stack.replace(ctx, len);
            Ok(CallbackReturn::Return)
        }),
    );

    string.set_field(
        ctx,
        "byte",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (string, i, j) = stack.consume::<(String, Option<i64>, Option<i64>)>(ctx)?;
            let i = i.unwrap_or(1);
            let substr = sub(string.as_bytes(), i, j.or(Some(i)));
            stack.extend(substr.iter().map(|b| Value::Integer(i64::from(*b))));
            Ok(CallbackReturn::Return)
        }),
    );

    string.set_field(
        ctx,
        "char",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let string = ctx.intern(
                &stack
                    .into_iter()
                    .map(|c| u8::from_value(ctx, c))
                    .collect::<Result<Vec<_>, _>>()?,
            );
            stack.replace(ctx, string);
            Ok(CallbackReturn::Return)
        }),
    );

    string.set_field(
        ctx,
        "sub",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (string, i, j) = stack.consume::<(String, i64, Option<i64>)>(ctx)?;
            let substr = ctx.intern(sub(string.as_bytes(), i, j));
            stack.replace(ctx, substr);
            Ok(CallbackReturn::Return)
        }),
    );

    string.set_field(
        ctx,
        "lower",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let string = stack.consume::<String>(ctx)?;
            let lowered = ctx.intern(
                &string
                    .as_bytes()
                    .iter()
                    .map(u8::to_ascii_lowercase)
                    .collect::<Vec<_>>(),
            );
            stack.replace(ctx, lowered);
            Ok(CallbackReturn::Return)
        }),
    );

    string.set_field(
        ctx,
        "reverse",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let string = stack.consume::<String>(ctx)?;
            let reversed = ctx.intern(&string.as_bytes().iter().copied().rev().collect::<Vec<_>>());
            stack.replace(ctx, reversed);
            Ok(CallbackReturn::Return)
        }),
    );

    string.set_field(
        ctx,
        "upper",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let string = stack.consume::<String>(ctx)?;
            let uppered = ctx.intern(
                &string
                    .as_bytes()
                    .iter()
                    .map(u8::to_ascii_uppercase)
                    .collect::<Vec<_>>(),
            );
            stack.replace(ctx, uppered);
            Ok(CallbackReturn::Return)
        }),
    );

    string.set_field(
        ctx,
        "rep",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            // TODO: fuel usage
            let (string, count) = stack.consume::<(String, i64)>(ctx)?;
            let repeated = string.repeat(count as usize);
            stack.replace(ctx, ctx.intern(&repeated));
            Ok(CallbackReturn::Return)
        }),
    );

    if matches!(std::env::var("STACK").as_deref(), Ok("1" | "true")) {
        load_pattern::<pattern::StackBackend>(ctx, string)
    } else {
        load_pattern::<pattern::SeqBackend>(ctx, string)
    }

    ctx.set_global("string", string);
}

pub fn load_pattern<'gc, F: pattern::FindBackend>(ctx: Context<'gc>, string: Table<'gc>) {
    string.set_field(
        ctx,
        "find",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            pattern::lua::lua_find::<F>(ctx, &mut stack)
        }),
    );

    string.set_field(
        ctx,
        "match",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            pattern::lua::lua_match::<F>(ctx, &mut stack)
        }),
    );

    string.set_field(
        ctx,
        "gmatch",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            pattern::lua::lua_gmatch::<F>(ctx, &mut stack)
        }),
    );

    string.set_field(ctx, "gsub", pattern::lua::lua_gsub_impl::<F>(ctx));
}

/// Convert a lua 1-indexed slice offset, which may be relative to the
/// string length, to a positive zero-indexed Rust index.  Note that the
/// index is *not* bounded to `len` when it is positive.
///
/// This can only fail on 32 bit platforms, where i64 values may not fit
/// into a usize.
fn convert_index(i: i64, len: usize) -> Option<usize> {
    let val = match i {
        0 => 0,
        v @ 1.. => v - 1,
        v @ ..=-1 => (len as i64 + v).max(0),
    };
    usize::try_from(val).ok()
}

fn convert_index_end(i: i64, len: usize) -> Option<usize> {
    let val = match i {
        v @ 0.. => v,
        v @ ..=-1 => (len as i64 + v + 1).max(0),
    };
    usize::try_from(val).ok()
}

fn sub(string: &[u8], i: i64, j: Option<i64>) -> &[u8] {
    let len = string.len();
    let i = convert_index(i, len).unwrap_or(usize::MAX);
    let j = convert_index_end(j.unwrap_or(len as i64), len)
        .unwrap_or(usize::MAX)
        .min(len);

    let slice = if i > j { &[] } else { &string[i..j] };
    slice
}
