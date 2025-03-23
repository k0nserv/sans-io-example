# Sans-IO example

This is an example of a simple program that uses the `sans-io` pattern. 

It's split across three crates

* `ping-core` Which implements the core logic in a sans-io way.
* `ping-sync` Which provides a binary around `ping-core` that uses synchronous IO.
* `ping-tokio` Which provides a binary around `ping-core` that uses Tokio.

## Building && Running

To build and run the synchronous version:

```sh
$ cargo build && sudo target/debug/ping-sync
```

and for the Tokio version:

```sh
$ cargo build && sudo target/debug/ping-tokio
```

## Slides

This example was used in a talk at [Rust Edinburgh](https://rustandfriends.org/) on the 20th of March 2025. 
The slides can be found in `slides.md` and viewed via [presenterm](https://github.com/mfontanini/presenterm)

