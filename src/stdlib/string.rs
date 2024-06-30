use std::borrow::Cow;

use crate::async_callback::{async_sequence, Locals};
use crate::meta_ops::MetaResult;
use crate::{
    meta_ops, Callback, CallbackReturn, Context, FromValue, IntoValue, SequenceReturn, Stack,
    StashedFunction, String, Table, Value,
};

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

    string.set_field(
        ctx,
        "find",
        Callback::from_fn(&ctx, |ctx, _, mut stack| pattern::lua_find(ctx, &mut stack)),
    );

    string.set_field(
        ctx,
        "match",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            pattern::lua_match(ctx, &mut stack)
        }),
    );

    string.set_field(
        ctx,
        "gmatch",
        Callback::from_fn(&ctx, |ctx, _, mut stack| {
            let (str, pat, init) = stack.consume::<(String, String, Option<i64>)>(ctx)?;
            // TODO: overflow checks on 32 bit
            let start = match init {
                Some(0) | None => 0,
                Some(v @ 1..) => (v - 1) as usize,
                Some(v @ ..=-1) => (str.len() + v).max(0) as usize,
            };

            let cur_idx = std::cell::Cell::new(start);
            let last_match_end = std::cell::Cell::new(None);

            let gmatch = Callback::from_fn_with(
                &ctx,
                (str, pat, (cur_idx, last_match_end)),
                |(str, pat, (cur_idx, last_match_end)), ctx, _, mut stack| {
                    stack.clear();
                    loop {
                        let start = cur_idx.get();
                        let res = pattern::str_find(&pat, &str, start, false)
                            .map_err(|_| "find error".into_value(ctx))?;

                        if let Some(m) = res {
                            if last_match_end.get() == Some(m.end) {
                                // TODO: does this work? (ref 6.4.1 mult matches)
                                cur_idx.set(m.end + 1);
                                continue;
                            } else {
                                cur_idx.set(m.end);
                            }
                            last_match_end.set(Some(m.end));
                            if m.captures.is_empty() {
                                stack.push_back(Value::String(ctx.intern(&str[m.start..m.end])));
                            } else {
                                stack.extend(m.captures.iter().map(|m| {
                                    if m.pos {
                                        Value::Integer(m.start as i64 + 1)
                                    } else {
                                        Value::String(ctx.intern(&str[m.start..m.end]))
                                    }
                                }));
                            }
                        } else {
                            cur_idx.set(str.as_bytes().len());
                            stack.push_back(Value::Nil);
                        }
                        break Ok(CallbackReturn::Return);
                    }
                },
            );

            stack.replace(ctx, gmatch);
            Ok(CallbackReturn::Return)
        }),
    );

    fn prep_metaop_call<'gc, const N: usize>(
        ctx: Context<'gc>,
        mut stack: Stack<'gc, '_>,
        locals: Locals<'gc, '_>,
        res: MetaResult<'gc, N>,
    ) -> Option<StashedFunction> {
        match res {
            MetaResult::Value(v) => {
                stack.push_back(v);
                None
            }
            MetaResult::Call(call) => {
                stack.extend(call.args);
                Some(locals.stash(&ctx, call.function))
            }
        }
    }

    let gsub_impl = Callback::from_fn(&ctx, |ctx, _, mut stack| {
        let (str, pat, repl, end) = stack.consume::<(String, String, Value, Option<i64>)>(ctx)?;
        let match_limit = end;
        let s = async_sequence(&ctx, |locals, mut seq| {
            let (str, pat, repl) = (
                locals.stash(&ctx, str),
                locals.stash(&ctx, pat),
                locals.stash(&ctx, repl),
            );
            async move {
                let mut match_count = 0;
                let mut cur_idx = 0;
                let mut last_end = None;
                let mut output_end = 0;
                let mut buffer: Option<Vec<u8>> = None;

                loop {
                    let match_res = seq.try_enter(|_ctx, locals, _exec, _stack| {
                        let (pat, str) = (locals.fetch(&pat), locals.fetch(&str));
                        let res = pattern::str_find(&pat, &str, cur_idx, false)?;
                        Ok(res)
                    })?;

                    let m = if let Some(m) = match_res {
                        m
                    } else {
                        break;
                    };

                    if last_end == Some(m.end) {
                        // TODO: does this work? (ref 6.4.1 mult matches)
                        cur_idx = m.end + 1;
                        continue;
                    } else {
                        cur_idx = m.end;
                    }
                    last_end = Some(m.end);

                    let first_capture = m
                        .captures
                        .get(0)
                        .map(|c| (c.start, c.end, c.pos))
                        .unwrap_or((m.start, m.end, false));

                    let func = seq.try_enter(|ctx, locals, _exec, mut stack| {
                        let (repl, str) = (locals.fetch(&repl), locals.fetch(&str));
                        Ok(match repl {
                            Value::String(repl_pattern) => {
                                let replaced = pattern::expand_substitution(
                                    &m,
                                    str.as_bytes(),
                                    repl_pattern.as_bytes(),
                                )?;
                                let string = match replaced {
                                    Cow::Borrowed(_) => repl_pattern, // unmodified
                                    Cow::Owned(bytes) => ctx.intern(&bytes),
                                };
                                stack.push_back(Value::String(string));
                                None
                            }
                            Value::Table(table) => {
                                let value = if !first_capture.2 {
                                    let substr = ctx.intern(&str[first_capture.0..first_capture.1]);
                                    Value::String(substr)
                                } else {
                                    Value::Integer(first_capture.0 as i64 + 1)
                                };
                                let res = meta_ops::index(ctx, Value::Table(table), value)?;
                                prep_metaop_call(ctx, stack, locals, res)
                            }
                            Value::Function(_) => {
                                let call = meta_ops::call(ctx, repl)?;
                                if m.captures.is_empty() {
                                    stack
                                        .push_back(Value::String(ctx.intern(&str[m.start..m.end])));
                                } else {
                                    stack.extend(m.captures.iter().map(|m| {
                                        if m.pos {
                                            Value::Integer(m.start as i64 + 1)
                                        } else {
                                            Value::String(ctx.intern(&str[m.start..m.end]))
                                        }
                                    }));
                                }
                                Some(locals.stash(&ctx, call))
                            }
                            _ => {
                                return Err("expected string, table, or function"
                                    .into_value(ctx)
                                    .into());
                            }
                        })
                    })?;

                    if let Some(func) = func {
                        seq.call(&func, 0).await?;
                    }

                    seq.try_enter(|ctx, locals, _exec, mut stack| {
                        let str = locals.fetch(&str);

                        let replacement = stack.consume::<Value>(ctx)?;

                        if let Value::Nil | Value::Boolean(false) = replacement {
                        } else {
                            let replacement = String::from_value(ctx, replacement)?;
                            let buf = match buffer.as_mut() {
                                None => {
                                    buffer = Some(str[0..m.start].to_owned());
                                    buffer.as_mut().unwrap()
                                }
                                Some(o) => {
                                    o.extend_from_slice(&str[output_end..m.start]);
                                    o
                                }
                            };
                            buf.extend_from_slice(replacement.as_bytes());
                            output_end = m.end;
                        }
                        Ok(())
                    })?;

                    match_count += 1;
                    if match_limit.is_some() && Some(match_count) >= match_limit {
                        break;
                    }
                }

                seq.enter(|ctx, locals, _exec, mut stack| {
                    let str = locals.fetch(&str);
                    let result = match buffer {
                        None => str,
                        Some(mut o) => {
                            o.extend_from_slice(&str[output_end..str.len() as usize]);
                            ctx.intern(&o)
                        }
                    };
                    stack.push_back(Value::String(result));
                    stack.push_back(Value::Integer(match_count));
                });

                Ok(SequenceReturn::Return)
            }
        });
        Ok(CallbackReturn::Sequence(s))
    });

    string.set_field(ctx, "gsub", gsub_impl);

    ctx.set_global("string", string);
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
