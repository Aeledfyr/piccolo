use crate::{meta_ops, BoxSequence, Callback, CallbackReturn, Context, Table, Thread, ThreadMode};

use super::base::PCall;

pub fn load_coroutine<'gc>(ctx: Context<'gc>) {
    let coroutine = Table::new(&ctx);

    coroutine
        .set(
            ctx,
            "create",
            Callback::from_fn(&ctx, |ctx, _, mut stack| {
                let thread = Thread::new(ctx);
                thread
                    .start_suspended(&ctx, meta_ops::call(ctx, stack.get(0))?)
                    .unwrap();
                stack.replace(ctx, thread);
                Ok(CallbackReturn::Return)
            }),
        )
        .unwrap();

    coroutine
        .set(
            ctx,
            "resume",
            Callback::from_fn(&ctx, |ctx, _, mut stack| {
                let thread: Thread = stack.from_front(ctx)?;
                Ok(CallbackReturn::Resume {
                    thread,
                    then: Some(BoxSequence::new(&ctx, PCall)),
                })
            }),
        )
        .unwrap();

    coroutine
        .set(
            ctx,
            "continue",
            Callback::from_fn(&ctx, |ctx, _, mut stack| {
                let thread: Thread = stack.from_front(ctx)?;
                Ok(CallbackReturn::Resume { thread, then: None })
            }),
        )
        .unwrap();

    coroutine
        .set(
            ctx,
            "status",
            Callback::from_fn(&ctx, |ctx, _, mut stack| {
                let thread: Thread = stack.consume(ctx)?;
                stack.replace(
                    ctx,
                    match thread.mode() {
                        ThreadMode::Stopped => "dead",
                        ThreadMode::Running | ThreadMode::Waiting => "running",
                        ThreadMode::Normal => "normal",
                        ThreadMode::Result | ThreadMode::Suspended => "suspended",
                    },
                );
                Ok(CallbackReturn::Return)
            }),
        )
        .unwrap();

    coroutine
        .set(
            ctx,
            "yield",
            Callback::from_fn(&ctx, |_, _, _| {
                Ok(CallbackReturn::Yield {
                    to_thread: None,
                    then: None,
                })
            }),
        )
        .unwrap();

    coroutine
        .set(
            ctx,
            "yieldto",
            Callback::from_fn(&ctx, |ctx, _, mut stack| {
                let thread: Thread = stack.from_front(ctx)?;
                Ok(CallbackReturn::Yield {
                    to_thread: Some(thread),
                    then: None,
                })
            }),
        )
        .unwrap();

    coroutine
        .set(
            ctx,
            "running",
            Callback::from_fn(&ctx, |ctx, exec, mut stack| {
                let current_thread = exec.current_thread();
                stack.replace(ctx, (current_thread.thread, current_thread.is_main));
                Ok(CallbackReturn::Return)
            }),
        )
        .unwrap();

    ctx.set_global("coroutine", coroutine).unwrap();
}
