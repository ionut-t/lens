# lens

A terminal UI for running and inspecting Vitest tests.

## Usage

```sh
# Run in current directory
lens

# Run against a Nx project
lens my-app
```

## Keybindings

| Key                 | Action                       |
| ------------------- | ---------------------------- |
| `j` / `k`           | Navigate                     |
| `h` / `l`           | Collapse / expand            |
| `H` / `L`           | Collapse all / expand all    |
| `Enter`             | Run selected test/suite/file |
| `a`                 | Run all                      |
| `r`                 | Rerun failed                 |
| `w`                 | Toggle watch mode            |
| `e`                 | Open in editor               |
| `y`                 | Yank path                    |
| `f` / `/`           | Filter                       |
| `{` / `}`           | Jump to prev/next file       |
| `[` / `]`           | Jump to prev/next error      |
| `Tab` / `Shift+Tab` | Switch panel                 |
| `q`                 | Quit                         |

## Install

```sh
cargo install --git https://github.com/ionut-t/lens
```
