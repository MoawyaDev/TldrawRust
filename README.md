# GPUII — Infinite Canvas

A lightweight desktop whiteboard for sketching ideas, mapping concepts, and organizing thoughts on an infinite canvas — built with Rust.

GPUII is a native desktop project made with **Rust** and **egui/eframe**. It focuses on a simple workflow: open a canvas, create shapes or notes, move things around, and save your work locally.

> **Project status:** Early development. The core canvas and file workflows are implemented, while several advanced whiteboard features are still planned.

## Features

- **Infinite canvas** with pointer-centered zoom and panning.
- **Drawing tools** for freehand strokes, lines, rectangles, and ellipses.
- **Sticky notes** for quick ideas and annotations.
- **Selection and movement** for canvas objects.
- **Eraser** for removing objects.
- **Style inspector** for changing colors, stroke width, and fill.
- **Multiple pages** within one document.
- **Undo and redo** with a bounded history.
- **Project files** saved and opened as JSON using the `.tldr` extension.
- **Export** to SVG and PNG.
- **Grid toggle** and a **Fit** action for bringing the canvas content into view.
- **Keyboard shortcuts** for common actions.

## Screenshots

Screenshots will be added here as the interface develops.

<!-- To add a screenshot, place it in the repository (for example, assets/screenshot.png)
     and uncomment the following line:
![GPUII screenshot](assets/screenshot.png)
-->

## Requirements

- [Rust toolchain](https://www.rust-lang.org/tools/install) with Cargo
- A supported desktop environment for the dependencies used by the project

## Build and run

Clone the repository, then run the app from the project directory:

```bash
git clone <repository-url>
cd gpuii
cargo run
```

Build a release version:

```bash
cargo build --release
```

Run the test suite and check formatting:

```bash
cargo test
cargo fmt --check
```

Replace `<repository-url>` with the URL of your GitHub repository. If the cloned folder has a different name, `cd` into that folder instead.

## Controls

| Action | Control |
|---|---|
| Select tool | `V` |
| Freehand draw | `D` |
| Eraser | `E` |
| Line tool | `L` |
| Sticky note tool | `N` |
| Undo | `Ctrl + Z` |
| Redo | `Ctrl + Y` |
| Save project | `Ctrl + S` |
| Open project | `Ctrl + O` |
| Delete selected object | `Delete` |
| Zoom | Mouse wheel |
| Pan canvas | Middle mouse button, or hold `Space` and drag |

On macOS, the app may use the platform's command-key conventions where supported.

## Saving and exporting

### GPUII project files

GPUII saves documents as JSON files with the `.tldr` extension. The format stores the document title, pages, camera state, element geometry, styles, and note text.

Despite the extension, **this is GPUII's own project format**. It does not currently claim compatibility with files created by the official tldraw application.

### SVG export

SVG export preserves the canvas artwork as vector markup and includes sticky-note text.

### PNG export

PNG export rasterizes the supported canvas geometry and sticky-note backgrounds. At the current stage, sticky-note text is **not yet rendered into PNG exports**. Export dimensions are limited to prevent excessively large allocations.

## Current limitations

GPUII is still being developed. The following features are not implemented in the current milestone:

- Lasso selection and multi-selection
- Frames and connector/arrow tools
- Snapping and alignment guides
- Editing text directly on the canvas
- Advanced fill patterns and stroke styles
- Theme and preference settings
- Camera inertia and animation
- Advanced diagnostics and debug overlays

These are future-work items, not features that the current version promises to provide.

## Technology

- **Rust** — application logic
- **egui / eframe** — native desktop UI and canvas interaction
- **Serde / serde_json** — document serialization
- **tiny-skia** — PNG rendering
- **rfd** — native file dialogs
- **UUID** — element identifiers

## Project structure

```text
.
├── Cargo.toml
├── Cargo.lock
├── README.md
└── src
    ├── app.rs       # Application UI, tools, and interaction handling
    ├── document.rs  # Document, page, and element data structures
    ├── export.rs    # SVG and PNG export
    ├── geometry.rs  # Canvas geometry and camera transforms
    ├── history.rs   # Undo/redo history
    ├── lib.rs
    └── main.rs      # Desktop application entry point
```

## Roadmap

Possible next steps for the project include:

- Multi-object selection and lasso selection
- Connectors, arrows, and frames
- Snap-to-grid and alignment guides
- Richer text editing and styling
- Theme preferences
- Improved PNG text rendering
- More export and document-format tests

The roadmap may change as the project evolves.

## Contributing

Issues, bug reports, and pull requests are welcome. Before submitting a change, please:

1. Keep the application native and compatible with the existing Rust/egui architecture unless a migration is discussed first.
2. Run `cargo fmt --check` and `cargo test`.
3. Describe what changed and how you tested it.
4. Be clear about any feature that is incomplete or not yet tested.

## License

The package metadata currently declares **MIT OR Apache-2.0**. Make sure the repository includes the corresponding `LICENSE-MIT` and `LICENSE-APACHE` files before presenting the project as fully licensed under those terms.

---

Built with Rust, with a focus on making visual thinking feel simple and local.
