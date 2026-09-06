<h1 align=center>
toipe
</h1>

<p align=center>
A trusty terminal typing tester.
</p>

<p align=center>
<a href="https://github.com/doriangironde/toipe/actions/workflows/lints-and-checks.yml"><img src="https://github.com/doriangironde/toipe/actions/workflows/lints-and-checks.yml/badge.svg" alt="Lints and checks badge" /></a>
</p>

<p align=center>
<img src=https://raw.githubusercontent.com/Samyak2/toipe/main/images/toipe.gif>
</p>

# Usage

## Install

### From GitHub Releases

Go to the [latest release](https://github.com/doriangironde/toipe/releases/latest) and download the archive for your platform (`toipe-linux-x86_64.tar.gz` for Linux, `toipe-macos-arm64.zip` for Mac). Extract it and run the `toipe` binary inside.

### From source

Clone this repository and build it with cargo:

```
git clone https://github.com/doriangironde/toipe && cd toipe
cargo build --release
./target/release/toipe
```

### From Cargo

Alternatively, if you have the `cargo` tool (part of the Rust toolchain) installed on your system, you can install directly from this repository:

```
cargo install --git https://github.com/doriangironde/toipe
```

## Run typing test

toipe looks best on a nice terminal (such as Alacritty) with color and style support.

If installed through `cargo`, run:
```
toipe
```

If built from source, run the built binary directly:
```
./target/release/toipe
```

## Keyboard shortcuts

See `toipe --help` for a list of keyboard shortcuts (the list can also be found [here](https://github.com/Samyak2/toipe/blob/main/src/config.rs#L10)).

## Show less or more text

To change the number of words shown in each test, use the `-n` flag (default: 30):

```
toipe -n 10
```

```
toipe -n 100
```

## Use a different word list

By default, a list of top 250 English words (`top250`) is used and random words are selected from it. See `toipe -h` for a list of available built-in word lists.

To use the OS provided word list instead, use:
```
toipe -w os
```
Note: the OS word list varies a lot from system to system and usually has more than 100,000 words. This can lead to difficult and esoteric words appearing in the test, reducing your typing speed.

You can provide your own word list too (Note: the word list must meet [these assumptions](https://docs.rs/toipe/latest/toipe/textgen/struct.RawWordSelector.html#assumptions)):
```
toipe -f /path/to/word/list
```

## Add punctuation to test

By default, only lowercase words are shown. To add punctuation and sentence case, use the `-p` flag:

```
toipe -p
```

## Timed tests

Run a test of a fixed duration in seconds, with a sliding word window:

```
toipe -t 60
```

## Statistics

Every completed test is saved to `~/.local/share/toipe/history`. View your stats (personal best, averages) with:

```
toipe --stats
```

# Platform support

- toipe was only tested on Linux and Mac OS. If you find any problems, please [open an issue](https://github.com/Samyak2/toipe/issues).
- Windows is not supported yet. Follow [this issue](https://github.com/Samyak2/toipe/issues/14) for updates. It should work on WSL though.

# License

MIT
