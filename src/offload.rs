// MIT/Apache2 License

/*

 *Sighs.* I was really hoping I wouldn't have to do this. I try to prevent my code from being opinionated,
 which is why I generally use smol primitives. Not only does smol provide compatibility with async-std, it
 has smaller packages. I like having the smaller packages. (In addition, I think I could, theoretically, swap
 it all out for Tokio machinery in a feature if I ever want to do it that way.)

 However, my hand is forced by one essential detail: some parts of the program need to call asynchronous
 functions on drop (specifcally, the DRI3 contexts and drawables). AsyncDrop isn't even a part of the library
 yet, let alone gated behind a feature (hell, there's discussion on whether or not it should be a thing in
 the first place). I can't guarantee if the code is running with an executor (even if it really, really
 should be), let alone which executor I'm running with. Therefore, I'm left with only one option: keep a
 static, small executor running on a standalone thread for the sole purpose of handling drops.

 Once the offload() function defined here is called for the first time, it spawns a thread called
 "breadglx-offload". The sole purpose of this thread is to run async functions: specifically, drop
 handles we can't run on the main thread. We create an AsyncExecutor on it that runs for the program's lifetime,
 running these tasks.

 The main downside of this, aside from the fact that we now have **three** executor constructs running
 in the background (async-io and blocking, plus this), is that the functions we pass into the executor have
 to be 'static. This means we have to move all of our stuff out of the object being dropped before we call
 offload(). That's going to be a headache, especially with how often raw pointers are a part of the equation.

 (In addition, I also take advantage of this to spawn hot futures inside of C function where I'm expected
  to block. Since we have it, it's better than just calling future::block_on, so might as well.)

 TODO: Have a "tokio" feature that, when enabled, tries to get a tokio::Handle and spawns the task on that.
 TODO: find any other way of doing this
*/

#![cfg(feature = "async")]

use async_executor::{Executor, Task};
use futures_lite::future;
use once_cell::sync::Lazy;
use std::{future::Future, panic, thread};

static OFFLOADER: Lazy<Executor<'static>> = Lazy::new(|| {
    // we spawn two threads. one handles the drop calls created in async contexts, the other handles
    // hot futures in FFI calls
    const THREAD_COUNT: usize = 2;

    // avoid an allocation, why not?
    const OFFLOADER_THREAD_NAMES: &[&'static str] = &["breadglx-offload-1", "breadglx-offload-2"];

    for i in 0..THREAD_COUNT {
        thread::Builder::new()
            .name(OFFLOADER_THREAD_NAMES[i])
            .spawn(|| loop {
                panic::catch_unwind(|| future::block_on(OFFLOADER.run(future::pending::<()>())))
                    .ok();
            })
            .expect("Unable to spawn offloader thread");
    }

    Executor::new()
});

#[inline]
pub(crate) fn offload(future: impl Future<Output = ()> + Send + 'static) {
    OFFLOADER.spawn(future).detach()
}

#[inline]
pub(crate) fn spawn<T: Send + 'static>(
    future: impl Future<Output = T> + Send + 'static,
) -> Task<T> {
    OFFLOADER.spawn(future)
}

#[inline]
pub(crate) fn unblock<T: Send + 'static, F: FnOnce() -> T + Send + 'static>(
    f: F,
) -> impl Future<Output = T> {
    blocking::unblock(f)
}
