# Tic-Tac-GPU

A simple _(cough cough)_ example on a tic-tac-toe game with
[wgpu](https://wgpu.rs/).

## Why?

Because I didn't find that many small applications which use wgpu as their
rendering interface, which is understandable with wgpu being quite verbose.
Nevertheless, I wanted to create one, also just to step into real water for the
first time without a tutorial.

## Installation

You can just use `cargo install` for that, assuming you installed Rust already
(If you didn't, check out [rustup](https://rustup.rs/)):

```console
cargo install --git https://github.com/MultisampledNight/tic-tac-gpu.git
```

This will by default install to `$HOME/.cargo/bin`, or whatever the equivalent
on your OS is. Either put that path on `$PATH`, or just run the `tic-tac-gpu`
binary directly.

## Totally asked questions

### Why are so many comments in `src/render.rs`, but almost none in `src/main.rs`?

These were more of a reminder to myself what components of wgpu actually do. I
happen to learn things better if I try to formulate them out.

### Why is the UI so ugly?

The final look didn't matter to me, more about the rendering itself. This is not
a full-featured super accessible tic-tac-toe game with multiple AI modes and
beautiful text rendering.

### Gamepad support?

See https://github.com/rust-windowing/winit/issues/944. TLDR: No one seems to
have time to implement it in winit, the windowing library I use, and I don't
know anything about X11, evdev, Wayland and Quartz, so I can't implement it
either.

### Screenshots?

![Screenshot showing a near-done game](https://user-images.githubusercontent.com/80128916/145604243-3b42a7e1-cb67-4497-9fdb-b354018f33bc.png)

_Was it really worth it?_

<!--
	vim:tw=80:
-->
