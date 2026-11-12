---
tags: [gamelib, design, ui, variant]
variant: D
status: draft-needs-review
---
# Variant D: Couch Mode

Back to [[15 Design Variants]] · [[00 Home]]

A **full-screen, controller-friendly** mode for playing on a TV, similar in spirit to the "big screen" modes some launchers have. It's meant to be **an extra mode** you switch into, not a replacement for the main UI.

![[gamelib-D1-couch-library.png]]

## How it works
- **Full screen**, with big text readable from across the room.
- The selected game's **art fills the background**, with its title, playtime and a big **Play** button on the left.
- A **horizontal row of covers** along the bottom. Left and right on the stick or D-pad moves through them, and the selected cover grows and gets a white outline.
- **Top tabs** (Home, Library, Collections, Search), switched with the shoulder buttons.
- **Button hints** bottom-right: **A** Play · **Y** Details · **X** Favourite · **B** Back.

## egui notes
- Full screen: `ViewportCommand::Fullscreen(true)`.
- **Controller input isn't built into egui.** The `gilrs` crate reads gamepads. Each frame, check for button presses and turn them into actions such as "move left" or "play".
- Without a controller, the same mode works with the **arrow keys + Enter + Esc**, so build keyboard support first and add `gilrs` after.
- The smooth "grow" on the selected cover: `ctx.animate_value_with_time(...)`.

## When to build it
After M8 (the final polish milestone), as a **stretch goal**. It reuses all the same data and the launcher, so it's mostly a new `ui/couch.rs` file. It would make a nice extra feature to show in a coursework demo.
