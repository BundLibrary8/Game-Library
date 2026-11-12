---
tags: [gamelib, design, ui, variant]
variant: B
status: draft-needs-review
---
# Variant B: Constellation

Back to [[15 Design Variants]] · [[00 Home]]

A deep-space theme. The whole window sits on a **star map**: a field of stars, soft purple and teal nebula glows, and faint lines joining stars into constellations. The panels are see-through "glass", so the stars show behind everything.

![[gamelib-B1-constellation-library.png]]
![[gamelib-B2-constellation-details.png]]

## What's different from variant A
- A **slim icon rail** (Home, Library, Stats, Settings) instead of the full sidebar, giving the covers more room.
- **Collections become filter chips** (Backlog, Co-op nights) beside All / Installed / Favourites.
- **Ice-blue to violet** gradient accent; the Play button is called **Launch** and glows.
- Game details shows **weekly playtime drawn as a constellation**: each week is a star, joined by lines.

## Colours
| Token | Value |
|---|---|
| Space background | `#060914` + purple glow `rgba(98,64,200,.45)` + teal glow `rgba(20,120,150,.35)` |
| Glass panel | `rgba(14,20,42,.55)`, border `rgba(150,180,255,.14)` |
| Stars | `#dfe8ff` at 25–90 % opacity |
| Constellation lines | `#8fb4ff` at 28 % opacity |
| Accent | ice `#7ad7ff` → violet `#b18cff` |

## Building the star background in egui
This is very doable, and a good learning exercise with `ui.painter()`:
1. **When the app starts**, generate about 250 stars once, with random position, size and brightness. Use a fixed random seed so they're the same every launch, and keep them in a `Vec<Star>`.
2. **Every frame**, before drawing the panels, paint the background with `painter.rect_filled(...)`, then a `painter.circle_filled(...)` for each star.
3. **Constellations** are small lists of points joined with `painter.line_segment(...)` in a faint blue.
4. **Nebula glow**: draw a few big circles with very low alpha on top of each other, or use a pre-made blurred PNG as the background image. The PNG is easier.
5. **Optional twinkle**: change each star's alpha with `sin(time + star.phase)` and call `ctx.request_repaint()`. Keep it subtle, and allow it to be **turned off in Settings** because it uses a little CPU.
6. **Glass panels**: egui can't blur what's behind a panel, but a `Frame` with a semi-transparent fill (`Color32::from_rgba_unmultiplied(14,20,42,140)`) and a thin border looks almost the same.

> [!tip] Theme, not layout
> This could be a **theme option** ("Dark" / "Constellation") rather than a separate layout. The star background and colours change, and the screens stay the same. That's less work than building two UIs.
