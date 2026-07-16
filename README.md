# lens

A terminal UI for running and inspecting Vitest tests.

## Usage

```sh
# Run in current directory
lens

# Run against a Nx project
lens my-app

# Run a specific file on startup
lens --file src/todos/todos.service.spec.ts

# Run a specific test/suite on startup (vitest -t pattern)
lens --file src/todos/todos.service.spec.ts --test "should create a todo"

# Same, but keep re-running on file changes
lens --file src/todos/todos.service.spec.ts --watch
```

### Options

| Flag                | Description                                          |
| ------------------- | ---------------------------------------------------- |
| `--file <PATH>`     | Run this test file on startup                        |
| `-t, --test <NAME>` | Run only tests/suites matching NAME (needs `--file`) |
| `-w, --watch`       | Start in watch mode                                  |
| `--hide-failed`     | Start with the failed-tests panel hidden             |
| `--layout <L>`      | Panel layout: `auto`, `horizontal` or `vertical`     |

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
| `v`                 | Cycle layout                 |
| `x`                 | Toggle failed-tests panel    |
| `q`                 | Quit                         |

The layout adapts to the terminal width: below 100 columns (e.g. inside an
editor split) the panels stack vertically so test output stays readable.
Press `v` to cycle auto → horizontal → vertical manually.

## Install

```sh
cargo install --git https://github.com/ionut-t/lens
```
